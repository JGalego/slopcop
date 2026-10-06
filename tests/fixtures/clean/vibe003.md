The robust mutex recovers ownership after a panic. This detail is important because poisoned state remains observable.

Send the API key in the `Authorization` header. Each key belongs to one project, and a revoked key
fails immediately. Rotate the key from the dashboard; the old key stays valid for one hour so that
running workers can pick up the new key.

Hold the lock only inside the critical section, and keep the critical path free of allocation.
