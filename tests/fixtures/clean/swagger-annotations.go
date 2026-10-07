package repo

// CreateIssue opens an issue in the repository named by the path.
func CreateIssue(ctx *Context) {
	// swagger:operation POST /repos/{owner}/{repo}/issues issue issueCreateIssue
	// ---
	// summary: Create an issue
	// consumes:
	// - application/json
	// produces:
	// - application/json
	// responses:
	//   "201":
	//     description: response when creating an issue
	//   "204":
	//     description: response when creating an issue
	ctx.Create()
}
