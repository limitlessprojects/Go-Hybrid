package xfer

import (
	"fmt"
	"math/big"
)

// The two oracles below are written without the plan and without the overlap
// formula. They are the only independent signal in this spike: the plan check and
// the plan builder share their notion of "overlap", so agreement between them is
// not evidence. Agreement between a plan's output and these functions is.

// IntegralResample evaluates the exact cumulative integral of a piecewise-constant
// source field at the destination edges and returns the destination cell averages.
//
// I(x) is computed by walking the source cell that contains x; the average over
// [a,b] is (I(b)-I(a))/(b-a). No overlap length is ever computed here.
func IntegralResample(src, dst *Mesh, srcVal []*big.Rat) ([]*big.Rat, error) {
	if src == nil || dst == nil {
		return nil, fmt.Errorf("%w: nil mesh", ErrMeshBuild)
	}
	if len(srcVal) != src.Cells() {
		return nil, fmt.Errorf("%w: %d values for %d source cells",
			ErrFieldLength, len(srcVal), src.Cells())
	}
	// cumulative integral at every source edge
	cum := make([]*big.Rat, src.Cells()+1)
	cum[0] = new(big.Rat)
	for i := 0; i < src.Cells(); i++ {
		step := new(big.Rat).Mul(srcVal[i], new(big.Rat).SetInt64(src.Width(i)))
		cum[i+1] = new(big.Rat).Add(cum[i], step)
	}
	at := func(x int64) (*big.Rat, error) {
		lo, hi := src.edges[0], src.edges[len(src.edges)-1]
		if x < lo || x > hi {
			return nil, fmt.Errorf("xfer: oracle asked for %d outside the domain [%d,%d]", x, lo, hi)
		}
		// binary search for i with edges[i] <= x <= edges[i+1]
		a, b := 0, len(src.edges)-1
		for a < b-1 {
			m := (a + b) / 2
			if src.edges[m] <= x {
				a = m
			} else {
				b = m
			}
		}
		frac := new(big.Rat).SetInt64(x - src.edges[a])
		return new(big.Rat).Add(cum[a], frac.Mul(frac, srcVal[a])), nil
	}
	out := make([]*big.Rat, dst.Cells())
	for j := 0; j < dst.Cells(); j++ {
		dLo, dHi := dst.Cell(j)
		iLo, err := at(dLo)
		if err != nil {
			return nil, err
		}
		iHi, err := at(dHi)
		if err != nil {
			return nil, err
		}
		delta := new(big.Rat).Sub(iHi, iLo)
		out[j] = new(big.Rat).Quo(delta, new(big.Rat).SetInt64(dHi-dLo))
	}
	return out, nil
}

// RasterResample is the oracle used in the issue text: raster the field on a unit
// grid and average the raster over each destination cell. Requires integer edges
// and a unit grid anchored at the domain start.
func RasterResample(src, dst *Mesh, srcVal []*big.Rat) ([]*big.Rat, error) {
	if len(srcVal) != src.Cells() {
		return nil, fmt.Errorf("%w: %d values for %d source cells",
			ErrFieldLength, len(srcVal), src.Cells())
	}
	origin := src.edges[0]
	span := src.edges[len(src.edges)-1] - origin
	raster := make([]*big.Rat, span)
	for i := 0; i < src.Cells(); i++ {
		w := src.Width(i)
		for k := int64(0); k < w; k++ {
			raster[src.edges[i]-origin+k] = srcVal[i]
		}
	}
	out := make([]*big.Rat, dst.Cells())
	for j := 0; j < dst.Cells(); j++ {
		dLo, dHi := dst.Cell(j)
		a, b := dLo-origin, dHi-origin
		if a < 0 || b > span {
			return nil, fmt.Errorf("xfer: raster oracle cannot reach [%d,%d]", dLo, dHi)
		}
		sum := new(big.Rat)
		for k := a; k < b; k++ {
			if raster[k] == nil {
				return nil, fmt.Errorf("xfer: raster hole at %d", k)
			}
			sum.Add(sum, raster[k])
		}
		out[j] = new(big.Rat).Quo(sum, new(big.Rat).SetInt64(b-a))
	}
	return out, nil
}

// IntegralOf returns the exact integral of a field on its own mesh.
func IntegralOf(m *Mesh, val []*big.Rat) (*big.Rat, error) {
	if m == nil {
		return nil, fmt.Errorf("%w: nil mesh", ErrMeshBuild)
	}
	return integralOver(m.edges, val)
}

// RatField builds a rational field from int64 densities.
func RatField(vals ...int64) []*big.Rat {
	out := make([]*big.Rat, len(vals))
	for i, v := range vals {
		out[i] = new(big.Rat).SetInt64(v)
	}
	return out
}

// ZeroField allocates a rational field of n cells.
func ZeroField(n int) []*big.Rat {
	out := make([]*big.Rat, n)
	for i := range out {
		out[i] = new(big.Rat)
	}
	return out
}

// FieldStrings renders a field for test messages.
func FieldStrings(vals []*big.Rat) []string {
	out := make([]string, len(vals))
	for i, v := range vals {
		if v == nil {
			out[i] = "<nil>"
			continue
		}
		out[i] = v.RatString()
	}
	return out
}

// EqualFields compares two rational fields exactly.
func EqualFields(a, b []*big.Rat) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] == nil || b[i] == nil {
			if a[i] != b[i] {
				return false
			}
			continue
		}
		if a[i].Cmp(b[i]) != 0 {
			return false
		}
	}
	return true
}
