The scanner reads each file once.

Rules share cached sentence and paragraph spans, so adding another density detector does not repeat segmentation work.

Reporters sort findings before writing output. The stable order makes snapshots and CI logs easy to compare.

Git staged mode reads index blobs rather than the worktree, preserving exactly what a commit would contain even after later edits.

The benchmark command measures complete scans and reports both throughput and elapsed time.