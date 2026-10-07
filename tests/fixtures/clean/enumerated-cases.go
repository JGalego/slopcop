package actions

// FirstRun returns the first step that has run, skipping the setup step.
func FirstRun(steps []Step) *Step {
	// The first step that has run, not counting setup. For example,
	// 1. setup(Success) -> build(Success) -> test(Running) -> deploy(Waiting): build.
	// 2. setup(Success) -> build(Skipped) -> test(Success) -> deploy(Success): test.
	// 3. setup(Success) -> build(Running) -> test(Waiting) -> deploy(Waiting): build.
	// 4. setup(Success) -> build(Skipped) -> test(Skipped) -> deploy(Skipped): none.
	// 5. setup(Success) -> build(Cancelled) -> test(Cancelled) -> deploy(Cancelled): none.
	for index := range steps {
		if steps[index].HasRun() {
			return &steps[index]
		}
	}
	return nil
}
