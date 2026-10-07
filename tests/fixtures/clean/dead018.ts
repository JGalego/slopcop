// Decoding first preserves byte offsets in validation errors.
parseInput();
// Persistence starts only after every record validates.
saveRecords();

// <https://html.spec.whatwg.org/multipage/forms.html#dom-form-requestsubmit>
function requestSubmit(form, submitter) {
  // Step 1. If submitter is not null, then validate it.
  validateSubmitter(form, submitter);
  // Step 2. Otherwise, set submitter to form.
  submit(form, submitter ?? form);
}
