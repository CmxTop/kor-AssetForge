package utils

import (
	"context"
	"testing"
	"time"

	"github.com/alicebob/miniredis/v2"
	"github.com/redis/go-redis/v9"
)

func TestInitRedis(t *testing.T) {
	// Start a mini redis server for testing
	mr, err := miniredis.Run()
	if err != nil {
		t.Fatalf("Failed to initialize miniredis: %v", err)
	}
	defer mr.Close()

	// Connect our client
	client, err := InitRedis(mr.Addr())
	if err != nil {
		t.Fatalf("Failed to InitRedis: %v", err)
	}
	defer client.Close()

	// Test Set and Get
	ctx := context.Background()
	testKey := "test:key"
	testVal := "hello world"

	err = client.Set(ctx, testKey, testVal, 5*time.Minute).Err()
	if err != nil {
		t.Errorf("Failed to set key: %v", err)
	}

	val, err := client.Get(ctx, testKey).Result()
	if err != nil {
		t.Errorf("Failed to get key: %v", err)
	}

	if val != testVal {
		t.Errorf("Expected %s, got %s", testVal, val)
	}

	// Test Del
	err = client.Del(ctx, testKey).Err()
	if err != nil {
		t.Errorf("Failed to del key: %v", err)
	}

	// Should not exist
	_, err = client.Get(ctx, testKey).Result()
	if err != redis.Nil {
		t.Errorf("Expected redis.Nil, got %v", err)
	}
}
