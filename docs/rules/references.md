# Reference sources

The `vibecheck` rules draw on public catalogs of AI writing patterns and on corpus studies that measured which words and structures language models overuse. This page records which source informed which rule, and which catalogued patterns slopcop leaves out and why.

A pattern appearing in a catalog is motivation, not evidence. Each rule still needs a defensible quiet case, as [the rule development guide](../contributing-rules.md) requires, so slopcop counts clusters and densities where a catalog bans single words.

## Catalogs and style guides

| Source | What it catalogs | Rules it informed |
| --- | --- | --- |
| [Wikipedia: Signs of AI writing](https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing) | The most detailed field guide, maintained by editors who review AI-written article drafts: inflated significance, copula avoidance, trailing participle analysis, vague attribution, challenges-and-outlook conclusions, collaborative phrasing, knowledge-cutoff hedging, and the citation markers and tracking parameters that chat interfaces leave in copied text | `VIBE019`, `VIBE023`, `VIBE024`, `VIBE025`, `VIBE027`, `VIBE028`, `VIBE029`, `VIBE033`, `VIBE034`, `VIBE035` |
| [Is It Slop? AI words and phrases](https://isitslop.io/ai-words-and-phrases/) | Overused verbs, connectives, abstract nouns, and sentence shapes such as "serves as" and "despite its X, it faces several challenges" | `VIBE001`, `VIBE023`, `VIBE028`, `VIBE034` |
| [Is It Slop? Signs of AI writing](https://isitslop.io/signs-of-ai-writing/) | Em dash rates, triads, paragraph symmetry, uniform section endings, chat citation debris | `VIBE007`, `VIBE011`, `VIBE018`, `VIBE027`, `VIBE034` |
| [Humanized Copy: AI vocabulary to avoid](https://humanizedcopy.com/posts/delve-tapestry-and-other-ai-vocabulary-to-avoid) | A word list with a plain replacement for each word | Replacement tables for `VIBE003`, `VIBE013`, `VIBE015`, `VIBE022`, `VIBE023` |
| [SlopDetector: AI words list](https://slopdetector.org/blog/ai-words-list) | Words in three tiers by strength, cliché openers, fake authority, business-speak, and buzzword collocations | `VIBE013`, `VIBE014`, `VIBE022`, `VIBE030`, `VIBE032`, `VIBE033` |
| [MegaDyne Systems: no AI slop writing rules](https://github.com/MegaDyneSystems/no-ai-slop-writing-rules-distilled/blob/main/no_ai_slop_prompt.md) | A writing prompt with banned verbs and their replacements, academic tells such as "prior to" and "in light of", intensifiers, weasel words, inflated symbolism, and chat markup | `VIBE004`, `VIBE015`, `VIBE016`, `VIBE025`, `VIBE027`, `VIBE030`, `VIBE031`, `VIBE032` |
| [Nous Research: anti-slop reference](https://github.com/NousResearch/autonovel/blob/master/ANTI-SLOP.md) | Tiered banned words, filler phrases, and structural red flags for fiction generation | `VIBE001`, `VIBE013`, `VIBE015`, `VIBE023` |
| [Chapter: how to edit AI-generated text](https://blog.chapter.pub/how-to-edit-ai-generated-text/) | An editor's search-and-replace list and the structural habits behind it | `VIBE013`, `VIBE023`, `VIBE030` |
| [Tessl blog-writer skill: AI writing anti-patterns](https://tessl.io/registry/jbaruch/blog-writer/1.1.50/files/skills/blog-writer/references/ai-anti-patterns.md) | A numbered catalog of anti-patterns with examples and repairs, including labeled rhetorical devices, false ranges, time-parasite openers, amplifier intensifiers, participle filler, and significance inflation | `VIBE001`, `VIBE021`, `VIBE025`, `VIBE028`, `VIBE029`, `VIBE031`, `VIBE032`, `VIBE033`, `VIBE034`, `VIBE035` |

## Corpus studies

The catalogs above cite these measurements, which decide whether a word belongs on a list.

| Study | Finding | Use in slopcop |
| --- | --- | --- |
| [Kobak et al., Science Advances, 2025](https://www.science.org/doi/10.1126/sciadv.adt3813), with [the excess-vocabulary data](https://github.com/berenslab/llm-excess-vocab) | Hundreds of style words rose sharply in PubMed abstracts after 2022; `delves` rose about 25-fold, and `showcasing` and `underscores` about 9-fold | Candidate words for `VIBE023`, `VIBE025`, and `VIBE030` |
| [Liang et al., 2024](https://arxiv.org/abs/2403.07183) | Adjectives such as `commendable`, `meticulous`, and `intricate` became 10 to 35 times more likely in AI conference peer reviews after ChatGPT's release | `VIBE023` |
| [Juzek and Ward, 2024](https://arxiv.org/abs/2412.11385) | Focal words such as `delve`, `intricate`, and `underscore` are overused, and preference tuning on human feedback is a plausible cause | `VIBE023` |
| [TextPulse: the vocabulary fingerprint of AI rewriting](https://textpulse.ai/research/ai-vocabulary-fingerprint) | Paired human and model texts: model rewrites use `thereby`, `utilized`, `constitutes`, `bolster`, and "serves as a" many times more often than the originals, and plain words such as `used`, `help`, and `people` far less; `delve` appears at nearly the same rate in both | `VIBE028` and `VIBE030`; the `delve` result is why a single word never triggers a rule |
| [GPTZero AI vocabulary](https://gptzero.me/ai-vocabulary) | Phrases ranked by how much more often they occur in AI documents than in human ones | Cross-check for the phrase lists |
| [slop-forensics](https://github.com/sam-paech/slop-forensics) | A toolkit that derives over-represented words, bigrams, and trigrams from model output | Method for deriving phrase lists from a corpus |
| [EQ-Bench Slop Score](https://eqbench.com/slop-score.html), with [its word and trigram lists](https://github.com/sam-paech/slop-score/tree/main/data) | A weighted score of slop words (60 percent), "not X, but Y" contrasts (25 percent), and slop trigrams (15 percent), from lists that slop-forensics derived from ten models' essays and stories | The two-sentence contrasts in `VIBE020`; the inflected forms in `VIBE014`, `VIBE022`, `VIBE023`, and `VIBE030`, taken from words that both this list and the Kobak et al. data flag |

## Pattern coverage

| Pattern | Example | Rule |
| --- | --- | --- |
| Chat citation markers and tracking parameters | `:contentReference[oaicite:0]{index=0}`, `turn0search0`, `[cite: 1]`, `?utm_source=chatgpt.com` | `VIBE027` |
| Copula avoidance | "serves as the entry point", "boasts a dozen options", "plays a pivotal role in" | `VIBE028` |
| Trailing participle commentary | "..., highlighting its commitment to quality" | `VIBE029` |
| Latinate and academic substitutes | `utilize`, `thereby`, "prior to", "in order to", "a plethora of" | `VIBE030` |
| Empty intensifiers | `truly`, `incredibly`, `dramatically`, `undeniably` | `VIBE031` |
| Time-parasite and scene-setting openers | "In today's fast-paced world", "Now more than ever", "Look no further" | `VIBE032` |
| Vague attribution | "Studies show", "Experts agree", "Industry reports" | `VIBE033` |
| Challenges-and-outlook conclusions | "Despite these challenges, it continues to thrive", "Only time will tell" | `VIBE034` |
| Emoji as headings and bullets | `## 🚀 Features` | `VIBE035` |
| Labeled devices and faux-insight setups | "Here's the kicker", "Plot twist", "What nobody tells you" | `VIBE001` |
| Additive connectives | "Moreover,", "Furthermore,", "Additionally," | `VIBE001` |
| Collaborative closings | "Would you like me to", "Let me know if you need" | `VIBE002` |
| Weasel hedges | "can potentially", "may be able to", "helps ensure" | `VIBE004` |
| Didactic disclaimers | "It goes without saying", "Needless to say" | `VIBE015` |
| Refusals and source-availability hedging | "I'm sorry, but I can't", "While specific details are limited" | `VIBE019` |
| False ranges and audience lists | "from beginners to experts", "Whether you're a developer or a designer" | `VIBE021` |
| Inflated symbolism | "a stark reminder", "underscores the importance of", "paves the way", "an indelible mark" | `VIBE024`, `VIBE025` |
| Overused AI vocabulary | `showcase`, `interplay`, `meticulous`, `vibrant`, `bolster`, `garner` | `VIBE023` |
| Contrast split across two sentences | "It's not a cache. It's a database." "It doesn't guess. It measures." | `VIBE020` |
| Business-speak | "low-hanging fruit", "circle back", "one-stop shop" | `VIBE022` |

## Replacements

Each source pairs the words it flags with plainer alternatives, and slopcop keeps those pairs with the rules. A rule that counts specific expressions lists a replacement for each one in its metadata. A finding names the replacements for the expressions it matched:

```text
README.md:3:1  VIBE030  info
observed: observed 7 inflated substitutes for plain words using 7 distinct forms; replacements: "in order to" -> to; "ascertain" -> find out, check; "utilized" -> used; "prior to" -> before; "thereby" -> so, which; "aforementioned" -> this, the
```

`slopcop explain RULE_ID` prints the full table, the HTML report and the editor hover list it under the rule, and the JSON rule metadata carries it as `replacements`. Many replacements are an instruction rather than a word, such as "cut it" or "give the number", because the plain fix for a filler phrase is to delete it and the fix for an unsupported adjective is the evidence it stands for.

## Patterns left out

Some catalogued patterns are not rules, because they are house style, need semantic judgment, or need information that a deterministic offline scan does not have.

- **Single banned words.** Most catalogs ban a word on sight. The TextPulse comparison found `delve` at nearly the same rate in human and model text, and words such as `robust` or `landscape` have literal technical senses, so every vocabulary rule requires several distinct forms at a minimum density.
- **A blanket em dash ban.** Human use of em dashes varies roughly fiftyfold between writers, according to the corpus figures that Is It Slop? cites, so `VIBE007` reports density rather than presence.
- **Title-case headings.** Several style guides, such as Chicago and APA, require them.
- **Bold inline-header lists.** Option references and feature lists use a bold term and a colon by design; `VIBE008` already exempts bold labels.
- **Typographic characters.** Curly quotes, the ellipsis character, and Unicode bullets come from word processors and keyboard layouts as often as from models.
- **Template placeholders.** Text such as `[Your Name]` belongs in issue and license templates, which repositories keep on purpose.
- **Synonym cycling, fabricated experience, euphemistic smoothing, and stacked statistics.** Judging these needs the meaning of the text, not its surface.
- **Unsourced numbers, invented citations, and broken links.** Checking them needs the network or outside knowledge; `VIBE033` covers the observable part, an appeal to unnamed authority.
- **Low perplexity.** Measuring it needs a language model; `VIBE010` and `VIBE018` measure the observable proxies, uniform sentence and paragraph length.
- **Technical homonyms.** `underscore`, `encapsulate`, `align`, and `integrates` name ordinary things in code documentation, so they are absent from the vocabulary lists or counted only as part of a phrase such as "underscores the importance of".
- **Fiction vocabulary.** Most of the EQ-Bench word and trigram lists come from generated stories: invented names, sound verbs, and trigrams such as "voice barely whisper". Repositories hold little narrative prose, so only the words that the Kobak et al. data also flags in nonfiction were adopted.
