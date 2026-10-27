/*
 * Peak-memory measurement harness (C nauty side) — the exact structural
 * counterpart of src/bin/memcheck.rs, for the paper's memory-usage figure.
 * Builds the same deterministic graph families (complete/cycle/path/star;
 * "random" is excluded here since exact RNG parity with Rust's SmallRng
 * isn't practical, so the memory comparison sticks to structural families)
 * and runs the same canonize-in-a-loop workload so `/usr/bin/time -l` (peak
 * RSS) is comparable to the Rust binary's measurement, one process per
 * (family, n) data point.
 *
 * Usage: c_memcheck <family> <n> [iters=2000]
 */
#include <nauty.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void build_complete(graph *g, int n, int m) {
    for (int i = 0; i < n; i++)
        for (int j = i + 1; j < n; j++)
            ADDONEEDGE(g, i, j, m);
}

static void build_cycle(graph *g, int n, int m) {
    if (n == 1) return;
    for (int i = 0; i < n; i++)
        ADDONEEDGE(g, i, (i + 1) % n, m);
}

static void build_path(graph *g, int n, int m) {
    for (int i = 0; i + 1 < n; i++)
        ADDONEEDGE(g, i, i + 1, m);
}

static void build_star(graph *g, int n, int m) {
    for (int i = 1; i < n; i++)
        ADDONEEDGE(g, 0, i, m);
}

int main(int argc, char **argv) {
    if (argc < 3) {
        fprintf(stderr, "usage: %s <family> <n> [iters]\n", argv[0]);
        return 1;
    }
    const char *family = argv[1];
    int n = atoi(argv[2]);
    int iters = argc > 3 ? atoi(argv[3]) : 2000;
    int m = SETWORDSNEEDED(n);

    DYNALLSTAT(graph, g, g_sz);
    DYNALLSTAT(graph, canong, canong_sz);
    DYNALLSTAT(int, lab, lab_sz);
    DYNALLSTAT(int, ptn, ptn_sz);
    DYNALLSTAT(int, orbits, orbits_sz);
    DYNALLOC2(graph, g, g_sz, n, m, "malloc");
    DYNALLOC2(graph, canong, canong_sz, n, m, "malloc");
    DYNALLOC1(int, lab, lab_sz, n, "malloc");
    DYNALLOC1(int, ptn, ptn_sz, n, "malloc");
    DYNALLOC1(int, orbits, orbits_sz, n, "malloc");

    EMPTYGRAPH(g, m, n);
    if (strcmp(family, "complete") == 0) build_complete(g, n, m);
    else if (strcmp(family, "cycle") == 0) build_cycle(g, n, m);
    else if (strcmp(family, "path") == 0) build_path(g, n, m);
    else if (strcmp(family, "star") == 0) build_star(g, n, m);
    else {
        fprintf(stderr, "unknown family '%s' (expected complete|cycle|path|star)\n", family);
        return 1;
    }

    DEFAULTOPTIONS_GRAPH(opts);
    opts.getcanon = TRUE;
    statsblk stats;

    long orbit_sum = 0;
    for (int i = 0; i < iters; i++) {
        densenauty(g, lab, ptn, orbits, &opts, &stats, m, n, canong);
        orbit_sum += stats.numorbits;
    }
    printf("%ld\n", orbit_sum);
    return 0;
}
