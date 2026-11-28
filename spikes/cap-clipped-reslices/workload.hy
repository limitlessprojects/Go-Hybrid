struct store { data [1024]int }
pub fn view(s []int, lo int, hi int) []int { return s[lo:hi] }
pub fn prefix(s []int, hi int) []int { return s[:hi] }
pub fn suffix(s []int, lo int) []int { return s[lo:] }
pub fn whole(s []int) []int { return s[:] }
pub fn array_view(s *store, lo int, hi int) []int { return s.data[lo:hi] }
pub fn pointer_view(s *[1024]int, lo int, hi int) []int { return s[lo:hi] }
pub fn nested(s []int, lo int, hi int) []int { return s[:hi][lo:] }
