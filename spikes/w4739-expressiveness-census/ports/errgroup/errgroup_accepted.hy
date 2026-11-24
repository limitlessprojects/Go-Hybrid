package errgroup_pilot

import "fmt"

struct fetch_outcome {
    index int
    data  string
    err   string
}

fn fetch_worker(index int, url string, ch chan fetch_outcome) {
    if url == "" {
        ch <- fetch_outcome{index: index, data: "", err: fmt.sprintf("empty url at index %d", index)}
        return
    }
    ch <- fetch_outcome{index: index, data: fmt.sprintf("fetched:%s", url), err: ""}
}

pub fn fetch_all_hybrid(urls []string) ([]string, error) {
    n := len(urls)
    if n == 0 {
        return []string{}, nil
    }

    ch := make(chan fetch_outcome, n)
    for i, url := range urls {
        task fetch_worker(i, url, ch)
    }

    results := make([]string, n)
    var first_err string = ""

    for k := 0; k < n; k++ {
        res := <-ch
        if res.err != "" && first_err == "" {
            first_err = res.err
        }
        results[res.index] = res.data
    }

    if first_err != "" {
        return nil, fmt.errorf("%s", first_err)
    }
    return results, nil
}
