package utils

import (
	"context"
	"fmt"
	"log"

	"github.com/redis/go-redis/v9"
)

// InitRedis initializes and returns a Redis client
func InitRedis(url string) (*redis.Client, error) {
	if url == "" {
		url = "localhost:6379" // Fallback local default
	}

	client := redis.NewClient(&redis.Options{
		Addr: url,
	})

	// Test the connection
	ctx := context.Background()
	_, err := client.Ping(ctx).Result()
	if err != nil {
		return nil, fmt.Errorf("failed to connect to Redis at %s: %w", url, err)
	}

	log.Printf("Successfully connected to Redis at %s", url)
	return client, nil
}
