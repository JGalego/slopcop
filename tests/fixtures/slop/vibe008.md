Input: read bytes. Output: return tokens. Limit: reject overflow. Cost: one allocation. The parser is deterministic. The scanner is parallel. Errors name the path. Tests cover failures.
The reason is simple: caching. The result: pages load in a third of the time. There is one problem: invalidation. What changed? The index moved to memory.
