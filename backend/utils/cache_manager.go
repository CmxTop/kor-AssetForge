package utils

import (
	"bytes"
	"compress/gzip"
	"context"
	"encoding/json"
	"errors"
	"io"
	"log"
	"sync"
	"sync/atomic"
	"time"

	"github.com/redis/go-redis/v9"
)

// ErrCacheMiss is returned when a key is not present in any cache tier.
var ErrCacheMiss = errors.New("cache miss")

// ---- singleflight (stdlib-only, no golang.org/x/sync dependency) ------------

type sfCall struct {
	wg  sync.WaitGroup
	val interface{}
	err error
}

type singleflightGroup struct {
	mu sync.Mutex
	m  map[string]*sfCall
}

func (g *singleflightGroup) Do(key string, fn func() (interface{}, error)) (interface{}, error, bool) {
	g.mu.Lock()
	if g.m == nil {
		g.m = make(map[string]*sfCall)
	}
	if c, ok := g.m[key]; ok {
		g.mu.Unlock()
		c.wg.Wait()
		return c.val, c.err, true // shared
	}
	c := &sfCall{}
	c.wg.Add(1)
	g.m[key] = c
	g.mu.Unlock()

	c.val, c.err = fn()
	c.wg.Done()

	g.mu.Lock()
	delete(g.m, key)
	g.mu.Unlock()

	return c.val, c.err, false
}

// ---- L1 in-memory cache entry -----------------------------------------------

type l1Entry struct {
	value     []byte
	expiresAt time.Time
}

func (e *l1Entry) expired() bool {
	return !e.expiresAt.IsZero() && time.Now().After(e.expiresAt)
}

// ---- CacheMetrics -----------------------------------------------------------

// CacheMetrics tracks hits, misses, and errors across both cache tiers.
type CacheMetrics struct {
	L1Hits   atomic.Int64
	L1Misses atomic.Int64
	L2Hits   atomic.Int64
	L2Misses atomic.Int64
	Errors   atomic.Int64
	Evictions atomic.Int64
}

// Snapshot returns a point-in-time copy suitable for JSON serialisation.
func (m *CacheMetrics) Snapshot() map[string]int64 {
	return map[string]int64{
		"l1_hits":   m.L1Hits.Load(),
		"l1_misses": m.L1Misses.Load(),
		"l2_hits":   m.L2Hits.Load(),
		"l2_misses": m.L2Misses.Load(),
		"errors":    m.Errors.Load(),
		"evictions": m.Evictions.Load(),
		"total_hits": m.L1Hits.Load() + m.L2Hits.Load(),
		"total_misses": m.L1Misses.Load() + m.L2Misses.Load(),
	}
}

// HitRate returns the overall cache hit rate as a float in [0, 1].
func (m *CacheMetrics) HitRate() float64 {
	hits := float64(m.L1Hits.Load() + m.L2Hits.Load())
	total := hits + float64(m.L1Misses.Load()+m.L2Misses.Load())
	if total == 0 {
		return 0
	}
	return hits / total
}

// ---- CacheManager -----------------------------------------------------------

// CacheManager implements a two-level (L1 in-memory, L2 Redis) cache with:
//   - Cache-aside pattern (read-through on miss)
//   - Write-through on Set
//   - Stampede prevention via singleflight
//   - Optional gzip compression for values above a size threshold
//   - TTL per key
//   - Hit/miss metrics
type CacheManager struct {
	redis       *redis.Client
	l1          sync.Map // map[string]*l1Entry
	sf          singleflightGroup
	Metrics     CacheMetrics
	compressMin int // bytes; values larger than this are gzip-compressed
}

// NewCacheManager creates a CacheManager backed by the provided Redis client.
// Pass nil to operate in L1-only mode.
func NewCacheManager(redisClient *redis.Client) *CacheManager {
	return &CacheManager{
		redis:       redisClient,
		compressMin: 1024, // compress values > 1 KB
	}
}

// Get implements the cache-aside read: L1 → L2 → miss.
func (cm *CacheManager) Get(ctx context.Context, key string) ([]byte, error) {
	// --- L1 ---
	if raw, ok := cm.l1.Load(key); ok {
		entry := raw.(*l1Entry)
		if !entry.expired() {
			cm.Metrics.L1Hits.Add(1)
			return cm.decompress(entry.value), nil
		}
		cm.l1.Delete(key)
		cm.Metrics.Evictions.Add(1)
	}
	cm.Metrics.L1Misses.Add(1)

	// --- L2 (Redis) ---
	if cm.redis != nil {
		val, err := cm.redis.Get(ctx, key).Bytes()
		if err == nil {
			cm.Metrics.L2Hits.Add(1)
			decompressed := cm.decompress(val)
			// Backfill L1 with a short TTL so repeat reads are fast.
			cm.l1.Store(key, &l1Entry{value: val, expiresAt: time.Now().Add(30 * time.Second)})
			return decompressed, nil
		}
		if !errors.Is(err, redis.Nil) {
			cm.Metrics.Errors.Add(1)
			log.Printf("CacheManager: Redis Get error for %s: %v", key, err)
		}
	}
	cm.Metrics.L2Misses.Add(1)
	return nil, ErrCacheMiss
}

// Set implements write-through: write to both L1 and L2 atomically.
func (cm *CacheManager) Set(ctx context.Context, key string, value []byte, ttl time.Duration) error {
	compressed := cm.compress(value)

	// L1
	cm.l1.Store(key, &l1Entry{value: compressed, expiresAt: time.Now().Add(ttl)})

	// L2 (Redis)
	if cm.redis != nil {
		if err := cm.redis.Set(ctx, key, compressed, ttl).Err(); err != nil {
			cm.Metrics.Errors.Add(1)
			log.Printf("CacheManager: Redis Set error for %s: %v", key, err)
			return err
		}
	}
	return nil
}

// Delete removes a key from both L1 and L2 (cache invalidation).
func (cm *CacheManager) Delete(ctx context.Context, keys ...string) error {
	for _, key := range keys {
		cm.l1.Delete(key)
	}
	if cm.redis != nil && len(keys) > 0 {
		if err := cm.redis.Del(ctx, keys...).Err(); err != nil {
			cm.Metrics.Errors.Add(1)
			return err
		}
	}
	return nil
}

// GetOrLoad implements the cache-aside pattern with stampede prevention.
// If the key is missing, loader is called exactly once even under concurrent
// requests for the same key (singleflight).
func (cm *CacheManager) GetOrLoad(ctx context.Context, key string, ttl time.Duration, loader func() (interface{}, error)) ([]byte, error) {
	if val, err := cm.Get(ctx, key); err == nil {
		return val, nil
	}

	val, err, _ := cm.sf.Do(key, func() (interface{}, error) {
		data, err := loader()
		if err != nil {
			return nil, err
		}
		jsonData, err := json.Marshal(data)
		if err != nil {
			return nil, err
		}
		_ = cm.Set(ctx, key, jsonData, ttl)
		return jsonData, nil
	})
	if err != nil {
		return nil, err
	}
	return val.([]byte), nil
}

// Invalidate removes all keys matching a prefix pattern from both tiers.
// L1 is scanned directly; Redis uses SCAN to avoid blocking.
func (cm *CacheManager) Invalidate(ctx context.Context, pattern string) error {
	// L1: iterate and delete matching keys
	cm.l1.Range(func(k, _ interface{}) bool {
		key := k.(string)
		if matchesPattern(pattern, key) {
			cm.l1.Delete(key)
			cm.Metrics.Evictions.Add(1)
		}
		return true
	})

	// L2: Redis SCAN + DEL
	if cm.redis != nil {
		var cursor uint64
		for {
			var keys []string
			var err error
			keys, cursor, err = cm.redis.Scan(ctx, cursor, pattern, 100).Result()
			if err != nil {
				cm.Metrics.Errors.Add(1)
				return err
			}
			if len(keys) > 0 {
				cm.redis.Del(ctx, keys...)
			}
			if cursor == 0 {
				break
			}
		}
	}
	return nil
}

// Warm pre-populates the cache for a list of (key, loader) pairs.
// Useful on application startup to avoid cold-start cache misses.
func (cm *CacheManager) Warm(ctx context.Context, entries []WarmEntry) {
	for _, e := range entries {
		if _, err := cm.Get(ctx, e.Key); err == nil {
			continue // already warm
		}
		data, err := e.Loader()
		if err != nil {
			log.Printf("CacheManager: warm loader error for %s: %v", e.Key, err)
			continue
		}
		jsonData, err := json.Marshal(data)
		if err != nil {
			continue
		}
		if setErr := cm.Set(ctx, e.Key, jsonData, e.TTL); setErr != nil {
			log.Printf("CacheManager: warm set error for %s: %v", e.Key, setErr)
		}
	}
}

// WarmEntry describes one key to pre-populate during cache warming.
type WarmEntry struct {
	Key    string
	TTL    time.Duration
	Loader func() (interface{}, error)
}

// ---- compression helpers ----------------------------------------------------

func (cm *CacheManager) compress(data []byte) []byte {
	if len(data) < cm.compressMin {
		return data
	}
	var buf bytes.Buffer
	w := gzip.NewWriter(&buf)
	if _, err := w.Write(data); err != nil {
		return data
	}
	if err := w.Close(); err != nil {
		return data
	}
	compressed := buf.Bytes()
	if len(compressed) >= len(data) {
		return data // compression didn't help
	}
	return compressed
}

func (cm *CacheManager) decompress(data []byte) []byte {
	if len(data) < 2 || data[0] != 0x1f || data[1] != 0x8b {
		return data // not gzip
	}
	r, err := gzip.NewReader(bytes.NewReader(data))
	if err != nil {
		return data
	}
	defer r.Close()
	out, err := io.ReadAll(r)
	if err != nil {
		return data
	}
	return out
}

// matchesPattern is a minimal glob matcher supporting only trailing '*'.
func matchesPattern(pattern, key string) bool {
	if len(pattern) == 0 {
		return key == pattern
	}
	if pattern[len(pattern)-1] == '*' {
		prefix := pattern[:len(pattern)-1]
		return len(key) >= len(prefix) && key[:len(prefix)] == prefix
	}
	return key == pattern
}
