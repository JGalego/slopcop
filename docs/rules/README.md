# Rule index

Every registered rule is deterministic. Defaults are conservative and can be changed under `[slopcop.rules]`. Run `slopcop explain RULE_ID` for examples, rationale, a suggestion, and complete false-positive notes.

Confidence describes how directly the rule observes its quality smell. It never estimates who wrote the artifact.

## deadweight

| Rule | Default | Confidence | Trigger | Quiet boundary |
| --- | --- | --- | --- | --- |
| `DEAD001` | error | high | Empty Python, JavaScript, or TypeScript exception handler | Behavior or an explanatory body comment documenting intentional omission |
| `DEAD002` | warning | high | `TODO`, `FIXME`, `HACK`, or `XXX` in code prose | Normal words and executable string literals; tracked debt can be suppressed with a reason |
| `DEAD003` | error | high | Executable unimplemented calls or raised exceptions | Caught exception types, mock references, and nearby abstract, trait, or protocol declarations |
| `DEAD004` | warning | high | An exception converted directly to `None`, `null`, `false`, or an empty collection | Narrow handling, translation, or an explanatory compatibility comment |
| `DEAD005` | warning | medium | A concrete function whose body is empty, `pass`, or an ellipsis | Recognized abstract/interface context or an explanatory hook comment |
| `DEAD006` | warning | medium | A short adjacent comment with near-complete token overlap with a simple operation | Comments that explain constraints, external behavior, ordering, or rationale |
| `DEAD007` | warning | high | The same substantial standalone line comment repeated beside itself | Inline annotations, type directives, distinct comments, and punctuation-only separators |
| `DEAD008` | info | medium | A function that forwards the same parameters to a same-named method and returns the result | Validation, conversion, policy, compatibility work, or different argument mapping |
| `DEAD009` | warning | high | Opposite Boolean literals returned from two branches of one conditional | Direct Boolean returns and branches with additional behavior |
| `DEAD010` | error | high | A test-path assertion that can only pass, such as `assert True` | Assertions over produced values or side effects |
| `DEAD011` | warning | medium | Ten substantial, identical, non-empty source lines repeated later in one file | Short scaffolding, blocks with literal differences, generated files, and intentionally distinct logic; each exact duplicate group reports once |
| `DEAD012` | warning | medium | An empty Markdown section or a short body that only restates its heading | Sections containing concrete prose, nested sections, lists, fenced examples, or decorative heading underlines |
| `DEAD013` | info | medium | An exact count of a mutable repository inventory such as rules, commands, or integrations | Release snapshots, generated summaries, compatibility limits, and fixed protocol cardinalities |
| `DEAD014` | warning | high | A Python handler that only uses bare `raise`, or a JS/TS handler that throws the same caught identifier | Exception translation, cleanup, recovery, or added context |

## papertrail

| Rule | Default | Confidence | Trigger | Quiet boundary |
| --- | --- | --- | --- | --- |
| `TRAIL001` | warning | high | A commit subject consisting only of a placeholder such as `WIP`, `fix`, or `update` | A specific subject that summarizes the observable change |
| `TRAIL002` | error | high | A `fixup!`, `squash!`, or `amend!` commit in inspected history | Commit-message hooks while an autosquash series is still being prepared |

## vibecheck

| Rule | Default | Confidence | Trigger | Quiet boundary |
| --- | --- | --- | --- | --- |
| `VIBE001` | warning | medium | At least four stock transitions at a density of one per 120 words or more | One ordinary transition or sparse use in long-form prose |
| `VIBE002` | warning | high | Two or more unquoted phrases that frame repository prose like an assistant response | Quoted examples, direct documentation, and isolated support language |
| `VIBE003` | warning | medium | Six generic evaluative modifiers at a density of one per 80 words or more | Isolated technical uses such as a robust mutex |
| `VIBE004` | warning | medium | Seven hedging markers at a density of one per 70 words or more | Calibrated uncertainty attached to a bounded technical claim |
| `VIBE005` | warning | medium | Two or more paired artificial-balance templates | A single contrast that affects a decision |
| `VIBE006` | warning | medium | Five explicit structure announcements at a density of one per 100 words or more | Necessary navigation in a long tutorial or specification |
| `VIBE007` | info | medium | At least five em dashes at a density of one per 100 words or more | Occasional editorial use |
| `VIBE008` | info | medium | Four or more colon sentences comprising at least 40 percent of eight substantial sentences | Field references, glossaries, and occasional definitions |
| `VIBE009` | warning | high | Four substantial sentences sharing the same first two words and forming at least half the sample | Short runs and intentional procedural parallelism |
| `VIBE010` | info | low | Eight substantial consecutive sentences whose word counts vary by no more than three | Natural variation, controlled language, or shorter runs |
| `VIBE011` | info | medium | Three separate exact triads that account for at least half of nonblank lines | One three-item list or a real three-part domain model |
| `VIBE012` | warning | medium | Eight vague demonstrative abstractions at a density of one per 80 words or more | A few unambiguous local references |
| `VIBE013` | warning | medium | Five generic metaphor markers using at least four forms, at one per 120 words or more | Literal discussion of maps, ecology, textiles, or travel |
| `VIBE014` | warning | medium | Six corporate positivity markers using at least four forms, at one per 100 words or more | Concrete outcome language and isolated domain terms |
| `VIBE015` | warning | medium | Four generic disclaimer phrases in one artifact | A specific caveat attached to the claim it changes |
| `VIBE016` | warning | high | Three unquoted conclusion markers, with at least two in the latter half | Quoted examples, one closing section, or independent chapter conclusions |
| `VIBE017` | warning | high | Adjacent substantial sentences with at least seven shared content words and 75 percent set overlap | Definitions and neighboring sentences that add distinct facts |
| `VIBE018` | info | low | Five substantial consecutive paragraphs whose word counts remain within a 20 percent band | Paragraphs sized by their evidence or a constrained publication format |
| `VIBE019` | warning | high | One unquoted chatbot identity, cutoff, or browsing-disclaimer artifact | Quoted examples and stored chat transcripts |

## Configuration

Override a default or disable one named rule:

```toml
[slopcop.rules]
VIBE010 = "warning"
DEAD008 = "off"
TRAIL001 = "error"
```

Unknown IDs are rejected. The self-hosting repository keeps every rule enabled and excludes only the deliberate positive fixture directory.