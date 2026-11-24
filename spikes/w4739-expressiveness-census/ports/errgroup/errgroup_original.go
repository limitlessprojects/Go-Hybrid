package errgroup_pilot

import (
	"context"
	"fmt"
	"sync"
)

type Group struct {
	cancel func()
	wg     sync.WaitGroup
	errOnce sync.Once
	err     error
}

func WithContext(ctx context.Context) (*Group, context.Context) {
	ctx, cancel := context.WithCancel(ctx)
	return &Group{cancel: cancel}, ctx
}

func (g *Group) Wait() error {
	g.wg.Wait()
	if g.cancel != nil {
		g.cancel()
	}
	return g.err
}

func (g *Group) Go(f func() error) {
	g.wg.Add(1)
	go func() {
		defer g.wg.Done()
		if err := f(); err != nil {
			g.errOnce.Do(func() {
				g.err = err
				if g.cancel != nil {
					g.cancel()
				}
			})
		}
	}()
}

// FetchAll demonstrates fan-out parallel fetch with shared error.
func FetchAll(urls []string) ([]string, error) {
	g, _ := WithContext(context.Background())
	results := make([]string, len(urls))

	for i, url := range urls {
		i, url := i, url
		g.Go(func() error {
			if url == "" {
				return fmt.Errorf("empty url at index %d", i)
			}
			results[i] = fmt.Sprintf("fetched:%s", url)
			return nil
		})
	}

	if err := g.Wait(); err != nil {
		return nil, err
	}
	return results, nil
}
