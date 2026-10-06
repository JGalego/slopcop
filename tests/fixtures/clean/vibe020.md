# Caching

The cache holds rendered pages for five minutes. It is a cache, not a database, so every entry can be rebuilt from the origin.

Eviction follows least-recently-used order. Measure the miss rate before changing the size, because each miss costs one origin request.

Style guides sometimes warn against "It's X, not Y" and "Rather than X, Y" when they are repeated.
