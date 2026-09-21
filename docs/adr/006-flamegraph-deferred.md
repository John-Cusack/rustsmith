# ADR-006: flame graphs deferred (no stack-data source)

The report gains a static SVG per-round-gain chart + unit DAG table instead.
No profiler in the pipeline captures stack samples; fabricating stacks
would be dishonest. Revisit when `perf`/cachegrind sampling is staged.
Proceed: no flame graphs; chart plots graded gains only.
