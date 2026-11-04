package decider

import (
	"go/importer"
	"go/types"
	"sync"
)

// CachedImporter caches loaded stdlib export packages in memory
// so repeated types.Check calls do not incur disk I/O.
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

// WarmStdlib pre-loads the commonly used stdlib packages into memory.
func (c *CachedImporter) WarmStdlib(pkgs ...string) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	for _, pkgPath := range pkgs {
		if _, ok := c.cache[pkgPath]; !ok {
			pkg, err := c.underlying.Import(pkgPath)
			if err != nil {
				return err
			}
			c.cache[pkgPath] = pkg
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
