package hybridge

import "testing"

func TestSPHeadroomPositive(t *testing.T) {
	if h := SPHeadroomForTest(); h == 0 {
		t.Fatal("SPHeadroomForTest() == 0")
	}
}
