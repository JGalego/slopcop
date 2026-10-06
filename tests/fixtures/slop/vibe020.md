# Caching

It's a cache, not a database. The goal isn't speed; it's predictability. Rather than tuning the size, measure the misses. The eviction policy is less about hit rate and more about tail latency.
