package decider

import (
	"go/importer"
	"go/types"
	"sync"
)

// CachedImporter caches loaded stdlib *types.Package instances in memory
// so repeated types.Check invocations execute with zero disk/export I/O.
type CachedImporter struct {
	underlying types.Importer
	mu         sync.RWMutex
	cache      map[string]*types.Package
}

func NewCachedImporter() *CachedImporter {
	return &CachedImporter{
		underlying: importer.Default(),
		cache:      make(map[string]*types.Package),
	}
}

func (c *CachedImporter) WarmStdlib(pkgs ...string) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	for _, p := range pkgs {
		if _, ok := c.cache[p]; !ok {
			pkg, err := c.underlying.Import(p)
			if err != nil {
				return err
			}
			c.cache[p] = pkg
		}
	}
	return nil
}

func (c *CachedImporter) Import(path string) (*types.Package, error) {
	c.mu.RLock()
	if pkg, ok := c.cache[path]; ok {
		c.mu.RUnlock()
		return pkg, nil
	}
	c.mu.RUnlock()

	c.mu.Lock()
	defer c.mu.Unlock()
	if pkg, ok := c.cache[path]; ok {
		return pkg, nil
	}
	pkg, err := c.underlying.Import(path)
	if err != nil {
		return nil, err
	}
	c.cache[path] = pkg
	return pkg, nil
}
