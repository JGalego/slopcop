// Validate headers before decoding the body.
// The body limit protects worker memory.
validate(request);

const keys = projectKeys(input); // (batch, tokens, groups, width)
const values = projectValues(input); // (batch, tokens, groups, width)

// assert.equal(escape("a?b"), "a[?]b");
// assert.equal(escape("a[b"), "a[[]b");
run();
