# Rule index

Every registered rule is deterministic. Defaults are conservative and can be changed under `[slopcop.rules]`. Run `slopcop explain RULE_ID` for examples, rationale, a suggestion, and complete false-positive notes.

Confidence describes how directly the rule observes its quality smell. It never estimates who wrote the artifact.

## deadweight

| Rule | Default | Confidence | Trigger | Quiet boundary |
| --- | --- | --- | --- | --- |
| `DEAD001` | error | high | Empty Python, JavaScript, or TypeScript exception handler | Behavior, an explanatory body comment, optional imports, exhausted iterators, or a guarded block that always raises |
| `DEAD002` | warning | high | `TODO`, `FIXME`, `HACK`, or `XXX` in code prose | Normal words, lowercase `xxx`/`hack`, code-quoted references, executable string literals, and test code; tracked debt can be suppressed with a reason |
| `DEAD003` | error | high | Executable unimplemented calls or raised exceptions | Caught exception types, mock references, nearby abstract, trait, or protocol declarations, Python methods whose whole body raises `NotImplementedError`, and conditional guards |
| `DEAD004` | warning | high | An exception converted directly to `None`, `null`, `false`, or an empty collection | Narrow handling, translation, an explanatory compatibility comment, optional imports, or the `false` answer of a predicate that also returns `true` |
| `DEAD005` | warning | medium | A concrete function whose body is empty, `pass`, or an ellipsis | Recognized abstract/interface context, an explanatory hook comment, decorated framework handlers, or test code |
| `DEAD006` | warning | medium | A short adjacent comment with near-complete token overlap with a simple operation | Comments that explain constraints, external behavior, ordering, or rationale; lines inside multi-line comments; and string data |
| `DEAD007` | warning | high | The same substantial standalone line comment repeated beside itself | Inline annotations, type directives, documentation comments, comments that differ in symbols, and punctuation-only separators |
| `DEAD008` | info | medium | A function that forwards the same parameters to a same-named method and returns the result | Validation, conversion, policy, compatibility work, different argument mapping, decorated handlers, or methods that expose a member object's operation |
| `DEAD009` | warning | high | Opposite Boolean literals returned from two branches of one conditional | Direct Boolean returns and branches with additional behavior |
| `DEAD010` | error | high | A test-path assertion that can only pass, such as `assert True` | Assertions over produced values or side effects |
| `DEAD011` | warning | medium | Ten substantial, identical, non-empty source lines repeated later in one file | Short scaffolding, blocks with literal differences, comments and embedded text, test code, generated files, and intentionally distinct logic; each exact duplicate group reports once |
| `DEAD012` | warning | medium | An empty Markdown section or a short body that only restates its heading | Sections containing concrete prose, nested sections, lists, fenced or indented examples, stacked headings that share one body, or decorative heading underlines |
| `DEAD013` | info | medium | An exact count of a mutable repository inventory such as rules, commands, or integrations | Release snapshots, generated summaries, compatibility limits, and fixed protocol cardinalities |
| `DEAD014` | warning | high | A Python handler that only uses bare `raise`, or a JS/TS handler that throws the same caught identifier | Exception translation, cleanup, recovery, or added context |
| `DEAD015` | warning | medium | A Python handler that only calls `print`, or a JS/TS handler that only calls `console.log`, `warn`, or `error` | Best-effort batch processing and interactive command loops that intentionally continue |
| `DEAD016` | error | high | A production JSX event property whose arrow callback has an empty body | Recognized test files; implemented callbacks |
| `DEAD017` | warning | medium | An action-named JS/TS function whose entire body returns `{ ok: true }` or `{ success: true }` | Result constructors, test factories, and functions that perform real work before returning success |
| `DEAD018` | warning | medium | At least two standalone line comments labeled with a numbered `Step` or `Phase`, including banner forms | One isolated phase label or a standardized algorithm whose phases carry domain meaning |

## papertrail

| Rule | Default | Confidence | Trigger | Quiet boundary |
| --- | --- | --- | --- | --- |
| `TRAIL001` | warning | high | A commit subject consisting only of a placeholder such as `WIP`, `fix`, or `update` | A specific subject that summarizes the observable change |
| `TRAIL002` | error | high | A `fixup!`, `squash!`, or `amend!` commit in inspected history | Commit-message hooks while an autosquash series is still being prepared |

## vibecheck

| Rule | Default | Confidence | Trigger | Quiet boundary |
| --- | --- | --- | --- | --- |
| `VIBE001` | warning | medium | At least four unquoted stock transitions or pivot phrases, such as "the real question is", "put differently", or "notably,", at a density of one per 120 words or more | One ordinary transition, sparse use in long-form prose, or quoted examples |
| `VIBE002` | warning | high | Two or more unquoted assistant-response phrases or standalone interjections such as "Absolutely!", using at least two forms | Quoted examples, direct documentation, interjection words inside a sentence, isolated support language, and one phrase repeated sparsely through a long guide |
| `VIBE003` | warning | medium | Six generic evaluative modifiers or importance labels, such as robust, crucial, or essential, at a density of one per 80 words or more | Isolated technical uses such as a robust mutex or a critical section; `key` is not counted because repositories use it as a noun |
| `VIBE004` | warning | medium | Seven hedging markers or padding phrases, such as "it can be argued" or "to some extent", at a density of one per 70 words or more | Calibrated uncertainty attached to a bounded technical claim |
| `VIBE005` | warning | medium | Two or more artificial-balance templates: paired one-hand/other-hand contrasts or stock both-sides lines such as "neither approach is inherently better" | A single contrast or comparison that affects a decision |
| `VIBE006` | warning | medium | Five explicit structure announcements, such as "first and foremost" or "there are three key points", at a density of one per 100 words or more | Necessary navigation in a long tutorial or specification |
| `VIBE007` | info | medium | At least five em dashes at a density of one per 100 words or more | Occasional editorial use |
| `VIBE008` | info | medium | Three unquoted staged reveals such as "The result: X." or a short question answered in the same paragraph, at one per 200 words or more; or four or more colon sentences comprising at least 40 percent of eight substantial sentences | Field references, glossaries, bare troubleshooting labels, occasional definitions, lead-ins that end with a colon, question headings, code comments, and release notes |
| `VIBE009` | warning | high | Four substantial sentences sharing the same first two words and forming at least half the sample | Short runs and intentional procedural parallelism |
| `VIBE010` | info | low | Eight substantial consecutive sentences whose word counts vary by no more than three | Natural variation, controlled language, shorter runs, list entries, runs interrupted by code examples, code comments, and release notes |
| `VIBE011` | info | medium | Three separate exact list triads that account for at least half of nonblank lines, or three inline triads of abstract qualities such as "clear, concise, and compelling" at one per 150 words or more | One three-item list, a real three-part domain model, inline series of concrete names or actions, or release notes |
| `VIBE012` | warning | medium | Eight vague demonstrative abstractions, such as "this approach", "this dynamic", or "that perspective", at a density of one per 80 words or more | A few unambiguous local references |
| `VIBE013` | warning | medium | Five generic metaphor markers or stock idioms, such as "landscape" or "double-edged sword", using at least four forms, at one per 120 words or more | Literal discussion of maps, ecology, textiles, or travel |
| `VIBE014` | warning | medium | Four promotional positivity markers, such as "exciting opportunity" or "strong foundation", using at least three forms, at one per 150 words or more | Concrete outcome language and isolated domain terms |
| `VIBE015` | warning | medium | Four generic disclaimer phrases, such as "results may vary" or "there is no one-size-fits-all answer", in one artifact | A specific caveat attached to the claim it changes |
| `VIBE016` | warning | high | Three unquoted conclusion markers, such as "in conclusion", "the key takeaway", or "this serves as a reminder", with at least two in the latter half | Quoted examples, one closing section, or independent chapter conclusions |
| `VIBE017` | warning | high | Adjacent substantial sentences with at least seven shared content words and 75 percent set overlap | Definitions, neighboring sentences that add distinct facts, sentences in different paragraphs or comments, list entries, and release notes |
| `VIBE018` | info | low | Five substantial consecutive paragraphs whose word counts remain within a 20 percent band | Paragraphs sized by their evidence, a constrained publication format, walkthrough steps separated by code examples, or code comments |
| `VIBE019` | warning | high | One unquoted chatbot identity, cutoff, or browsing-disclaimer artifact | Quoted examples and stored chat transcripts |
| `VIBE020` | warning | medium | Three unquoted rhetorical contrast templates of at least two kinds, such as "it's X, not Y" or "the goal isn't X; it's Y", at a density of one per 250 words or more | One or two contrasts, one template repeated, quoted examples, and release notes |
| `VIBE021` | warning | medium | Two unquoted not-only/but-also or everything-from/to constructions, or four correlatives including one of those at a density of one per 150 words or more | Plain both/and and whether/or clauses, one emphatic pair, quoted examples, and release notes |
| `VIBE022` | warning | medium | Six workplace jargon markers, such as "leverage", "key stakeholders", or "drive alignment", using at least four forms, at one per 100 words or more | Isolated domain terms and concrete descriptions of who does what |
| `VIBE023` | warning | medium | Five generic AI-favored words, such as nuanced, holistic, or delve, using at least three forms, at one per 120 words or more | Common technical terms such as context, pattern, and edge case; one word repeated; or sparse use in long prose |

## Configuration

Override a default or disable one named rule:

```toml
[slopcop.rules]
VIBE010 = "warning"
DEAD008 = "off"
TRAIL001 = "error"
```

Unknown IDs are rejected. The self-hosting repository keeps every rule enabled and excludes only the deliberate positive fixture directory.
