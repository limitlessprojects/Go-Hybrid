package merge

type Live struct{}
type Moved struct{}

type Reducer[M any] interface {
	Identity() M
	Combine(acc M, shard M) M
}

type Capsule[R any, M any, S any] struct {
	reducer Reducer[M]
	shards  []M
}

type Shard[R any, M any, S any] struct {
	parent  *Capsule[R, M, Live]
	slotIdx int
}

func NewCapsule[R any, M any](r Reducer[M]) *Capsule[R, M, Live] {
	return &Capsule[R, M, Live]{reducer: r}
}

func Split[R any, M any](c *Capsule[R, M, Live]) *Shard[R, M, Live] {
	idx := len(c.shards)
	c.shards = append(c.shards, c.reducer.Identity())
	return &Shard[R, M, Live]{parent: c, slotIdx: idx}
}

func Mutate[R any, M any](s *Shard[R, M, Live], fn func(*M)) { fn(&s.parent.shards[s.slotIdx]) }
func Finish[R any, M any](s *Shard[R, M, Live]) *Shard[R, M, Moved] {
	return &Shard[R, M, Moved]{parent: s.parent, slotIdx: s.slotIdx}
}

func Settle[R any, M any](c *Capsule[R, M, Live]) (M, *Capsule[R, M, Moved]) {
	if len(c.shards) == 0 {
		return c.reducer.Identity(), &Capsule[R, M, Moved]{reducer: c.reducer}
	}
	lvl := append([]M(nil), c.shards...)
	for len(lvl) > 1 {
		nxt := make([]M, (len(lvl)+1)/2)
		for i := 0; i < len(lvl)/2; i++ {
			nxt[i] = c.reducer.Combine(lvl[2*i], lvl[2*i+1])
		}
		if len(lvl)%2 == 1 {
			nxt[len(nxt)-1] = lvl[len(lvl)-1]
		}
		lvl = nxt
	}
	return lvl[0], &Capsule[R, M, Moved]{reducer: c.reducer}
}

type Histogram struct {
	Buckets [8]uint64
	Tags    map[string]uint64
}
type HistogramReducer struct{}

func (HistogramReducer) Identity() Histogram { return Histogram{Tags: make(map[string]uint64)} }
func (HistogramReducer) Combine(acc, shard Histogram) Histogram {
	out := Histogram{Tags: make(map[string]uint64, len(acc.Tags)+len(shard.Tags))}
	for i := 0; i < 8; i++ {
		out.Buckets[i] = acc.Buckets[i] + shard.Buckets[i]
	}
	for k, v := range acc.Tags {
		out.Tags[k] += v
	}
	for k, v := range shard.Tags {
		out.Tags[k] += v
	}
	return out
}

func VerifyCommutativeAssociative[M any](r Reducer[M], eq func(M, M) bool, probes []M) bool {
	id := r.Identity()
	for _, a := range probes {
		if !eq(r.Combine(id, a), a) || !eq(r.Combine(a, id), a) {
			return false
		}
		for _, b := range probes {
			if !eq(r.Combine(a, b), r.Combine(b, a)) {
				return false
			}
			for _, c := range probes {
				if !eq(r.Combine(r.Combine(a, b), c), r.Combine(a, r.Combine(b, c))) || !eq(r.Combine(r.Combine(a, b), c), r.Combine(r.Combine(a, c), b)) {
					return false
				}
			}
		}
	}
	return true
}
