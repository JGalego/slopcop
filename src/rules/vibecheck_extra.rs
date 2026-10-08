use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use aho_corasick::AhoCorasick;
use regex::Regex;

use super::{BuiltinRule, Rule, ScanContext, emit};
use crate::analysis::{Span, links_specification, word_count, words};
use crate::language::SourceType;
use crate::model::{Confidence, Module, RuleMetadata, Severity};

static VIBE002: RuleMetadata = RuleMetadata {
    id: "VIBE002",
    module: Module::Vibecheck,
    description: "Assistant-like framing cluster",
    default_severity: Severity::Warning,
    default_confidence: Confidence::High,
    message: "The prose repeatedly addresses the reader like a chat assistant.",
    suggestion: "Remove conversational service framing and state the useful information directly.",
    rationale: "Phrases that offer help, praise the question, agree with the reader, or announce a guided tour are usually interface residue rather than repository documentation.",
    examples: &[
        "Great question. Let's dive in. Feel free to ask for more.",
        "Absolutely! Here's a breakdown. Let me know if you'd like more detail.",
    ],
    false_positives: "Double-quoted examples and one phrase repeated sparsely through a long guide are excluded; unquoted chat transcripts and support templates may still need configuration.",
    replacements: &[],
};

static VIBE003: RuleMetadata = RuleMetadata {
    id: "VIBE003",
    module: Module::Vibecheck,
    description: "Generic evaluative modifier density",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "Generic evaluative modifiers are unusually dense.",
    suggestion: "Replace generic praise or emphasis with the property, measurement, or tradeoff that matters.",
    rationale: "Repeated words such as robust, crucial, or essential label information as important instead of showing why it matters; isolated technical uses do not trigger this rule.",
    examples: &["A robust, powerful, comprehensive, crucial, seamless, and effective solution."],
    false_positives: "Product copy intentionally uses evaluative language more often than technical documentation.",
    replacements: MODIFIERS,
};

static VIBE004: RuleMetadata = RuleMetadata {
    id: "VIBE004",
    module: Module::Vibecheck,
    description: "Excessive hedging",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "Hedging language is unusually dense.",
    suggestion: "Keep uncertainty that reflects evidence, and remove generic caveats that do not change the claim.",
    rationale: "Layered may, might, perhaps, and generally clauses can make prose evasive without making uncertainty more precise.",
    examples: &["Perhaps this may generally be useful and might often help in many cases."],
    false_positives: "Scientific and risk-sensitive writing may require sustained, calibrated uncertainty.",
    replacements: &[],
};

static VIBE005: RuleMetadata = RuleMetadata {
    id: "VIBE005",
    module: Module::Vibecheck,
    description: "Repeated artificial balance",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "The prose repeatedly manufactures balance between positions.",
    suggestion: "State the position the evidence supports, and present alternatives only when they change the decision.",
    rationale: "One-hand/other-hand pairs and stock both-sides lines create an appearance of nuance and turn a clear opinion into a falsely balanced discussion.",
    examples: &[
        "There are valid arguments on both sides. Both approaches have their advantages and disadvantages.",
        "On the one hand, it is fast. On the other hand, it is small. On the one hand, it is new. On the other hand, it is tested.",
    ],
    false_positives: "Comparative analysis may legitimately weigh several named alternatives.",
    replacements: &[],
};

static VIBE006: RuleMetadata = RuleMetadata {
    id: "VIBE006",
    module: Module::Vibecheck,
    description: "Excessive explicit signposting",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "The document repeatedly announces its own structure.",
    suggestion: "Use headings where navigation helps and delete transitions that merely announce the next sentence.",
    rationale: "Dense firstly, in-this-section, and to-summarize markers spend words describing the document instead of the subject.",
    examples: &[
        "Firstly ... Secondly ... In this section ... As discussed above ... To summarize ...",
    ],
    false_positives: "Long tutorials and formal specifications can need more navigational language.",
    replacements: &[],
};

static VIBE007: RuleMetadata = RuleMetadata {
    id: "VIBE007",
    module: Module::Vibecheck,
    description: "Em dash density",
    default_severity: Severity::Info,
    default_confidence: Confidence::Medium,
    message: "Em dashes are unusually dense in this prose.",
    suggestion: "Use sentence boundaries, commas, or parentheses where the interruption does not need emphasis.",
    rationale: "Frequent em dashes create a repetitive rhetorical cadence; a few ordinary uses never trigger this rule.",
    examples: &["Five or more em dashes in a short passage."],
    false_positives: "An established editorial style may deliberately prefer em dashes.",
    replacements: &[],
};

static VIBE008: RuleMetadata = RuleMetadata {
    id: "VIBE008",
    module: Module::Vibecheck,
    description: "Colon-heavy sentence pattern",
    default_severity: Severity::Info,
    default_confidence: Confidence::Medium,
    message: "A large share of sentences use the same colon-led structure.",
    suggestion: "Keep colons for genuine expansions and vary sentences that do not introduce a list or definition.",
    rationale: "Repeated label: explanation sentences and staged reveals such as \"The result: X.\" or \"What changed? X.\" make prose look organized and dramatic while flattening every idea into the same template.",
    examples: &[
        "Input: one value. Output: one value. Result: one value. Reason: one value.",
        "The reason is simple: caching. The result: faster pages. What changed? The index.",
    ],
    false_positives: "Glossaries and field-reference documents naturally use many colons; release notes, code comments, lead-ins that end with a colon, and record lines such as log or validator output where three or more consecutive lines open with the same `Label:` prefix are excluded.",
    replacements: &[],
};

static VIBE009: RuleMetadata = RuleMetadata {
    id: "VIBE009",
    module: Module::Vibecheck,
    description: "Repetitive sentence openings",
    default_severity: Severity::Warning,
    default_confidence: Confidence::High,
    message: "Many sentences begin with the same words.",
    suggestion: "Combine closely related claims or make each sentence's subject explicit.",
    rationale: "Repeated sentence prefixes are a measurable form of generated rhythm and often reveal unnecessary restatement.",
    examples: &[
        "This system reads files. This system checks prose. This system prints findings. This system exits.",
    ],
    false_positives: "Procedures and intentionally parallel rhetoric may repeat openings for clarity or effect. Record lines, such as log or validator output where three or more consecutive lines open with the same `Label:` prefix, are not counted, nor are the comments of source files that link a WHATWG, W3C, TC39, or IETF specification, which quote its algorithm. Release notes repeat one entry template by design, and comment lines that open with a code expression, such as `1. preStep(Success) -> step1(Running)`, show cases rather than sentences.",
    replacements: &[],
};

static VIBE010: RuleMetadata = RuleMetadata {
    id: "VIBE010",
    module: Module::Vibecheck,
    description: "Uniform sentence rhythm",
    default_severity: Severity::Info,
    default_confidence: Confidence::Low,
    message: "A long run of sentences has unusually uniform length.",
    suggestion: "Combine or split sentences according to the ideas rather than preserving a repeated cadence.",
    rationale: "Eight similarly sized sentences in sequence can indicate templated prose, but the observation is intentionally low confidence.",
    examples: &["Eight consecutive sentences whose word counts vary by at most three."],
    false_positives: "Controlled-language documentation and material written for early readers often targets uniform sentence length; lists, tables, record lines such as log or validator output where three or more consecutive lines open with the same `Label:` prefix, code comments, and release notes are excluded.",
    replacements: &[],
};

static VIBE011: RuleMetadata = RuleMetadata {
    id: "VIBE011",
    module: Module::Vibecheck,
    description: "Repeated three-item list structure",
    default_severity: Severity::Info,
    default_confidence: Confidence::Medium,
    message: "The document is dominated by repeated three-item structures.",
    suggestion: "Group items by the actual shape of the material instead of forcing each section or sentence into a triad.",
    rationale: "One three-item list is normal; repeated exact triads across most of a document, or a run of abstract inline triads such as \"clear, concise, and compelling\", are a structural fingerprint worth reviewing.",
    examples: &[
        "Three separate list groups, each containing exactly three items.",
        "The tool is simple, practical, and effective. It brings speed, reliability, and flexibility. Docs stay clear, concise, and compelling.",
    ],
    false_positives: "Reference material organized around a real three-part model can legitimately repeat triads; inline triads of concrete names or actions are not counted, and release notes are excluded. A list with nested items is an outline rather than a triad, as in an option reference that lists a description and an example under each option.",
    replacements: &[],
};

static VIBE012: RuleMetadata = RuleMetadata {
    id: "VIBE012",
    module: Module::Vibecheck,
    description: "Vague abstraction density",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "Vague this/that abstractions are unusually dense.",
    suggestion: "Name the specific component, decision, constraint, or operation being discussed.",
    rationale: "Repeated this approach, this framework, and that context references can hide what each sentence is actually about.",
    examples: &["This approach improves that process within this framework and that context."],
    false_positives: "A tightly scoped paragraph may have an unambiguous local referent for several such phrases.",
    replacements: &[],
};

static VIBE013: RuleMetadata = RuleMetadata {
    id: "VIBE013",
    module: Module::Vibecheck,
    description: "Generic metaphor cluster",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "Several generic technology metaphors are clustered together.",
    suggestion: "Describe the concrete system behavior instead of its landscape, journey, ecosystem, or transformative potential.",
    rationale: "A cluster of interchangeable metaphors increases polish without increasing technical information.",
    examples: &[
        "Navigate the landscape, unlock the ecosystem, and begin a transformative journey.",
    ],
    false_positives: "A document about maps, ecology, textiles, or travel may use these words literally.",
    replacements: METAPHORS,
};

static VIBE014: RuleMetadata = RuleMetadata {
    id: "VIBE014",
    module: Module::Vibecheck,
    description: "Generic corporate positivity",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "Corporate positivity language is unusually dense.",
    suggestion: "Replace promotional characterizations with the observable outcome, owner, and constraint.",
    rationale: "Repeated exciting-opportunity, strong-foundation, and drive-success claims make neutral information sound promotional and pad technical prose with untestable value language.",
    examples: &[
        "This exciting opportunity builds a strong foundation, delivers valuable insights, and leaves us well-positioned to drive success.",
    ],
    false_positives: "Marketing pages are expected to use more benefit-oriented language than engineering references.",
    replacements: POSITIVITY,
};

static VIBE015: RuleMetadata = RuleMetadata {
    id: "VIBE015",
    module: Module::Vibecheck,
    description: "Disclaimer cluster",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "Generic disclaimers are clustered in a short document.",
    suggestion: "Keep caveats that change a decision and attach each one to the claim it qualifies.",
    rationale: "Repeated note-that, keep-in-mind, and results-may-vary phrases often protect generic prose without communicating a bounded risk.",
    examples: &[
        "It is important to note ... Keep in mind ... Results may vary ... As with any solution ...",
    ],
    false_positives: "Safety, legal, and medical material can require multiple explicit disclaimers. Release notes are skipped, since separate entries can each say a fix applies \"in some cases\".",
    replacements: DISCLAIMERS,
};

static VIBE016: RuleMetadata = RuleMetadata {
    id: "VIBE016",
    module: Module::Vibecheck,
    description: "Repeated conclusions",
    default_severity: Severity::Warning,
    default_confidence: Confidence::High,
    message: "The document repeatedly signals that it is concluding.",
    suggestion: "Keep one conclusion and remove intermediate summaries that repeat established claims.",
    rationale: "Multiple in-summary and in-conclusion turns are a strong sign that prose has been expanded beyond its information content.",
    examples: &["In summary ... In conclusion ... All in all ..."],
    false_positives: "Double-quoted examples are excluded; a compiled document containing several independent chapters may still have a conclusion per chapter.",
    replacements: &[],
};

static VIBE017: RuleMetadata = RuleMetadata {
    id: "VIBE017",
    module: Module::Vibecheck,
    description: "Adjacent lexical restatement",
    default_severity: Severity::Warning,
    default_confidence: Confidence::High,
    message: "Adjacent sentences repeat nearly the same content words.",
    suggestion: "Keep the more precise sentence or make the second sentence add a distinct fact.",
    rationale: "High lexical overlap between neighboring substantial sentences is an explainable proxy for unnecessary restatement.",
    examples: &[
        "The scanner reads every tracked source file in parallel. Every tracked source file is read in parallel by the scanner.",
    ],
    false_positives: "Definitions may intentionally restate a term once in equivalent language; list entries, table rows, release notes, record lines such as validator output or `<param>` docs where three or more consecutive lines open with the same two words, comments in source files that link a WHATWG, W3C, TC39, or IETF specification, which quote its algorithm, and sentences in separate paragraphs or comments are not compared.",
    replacements: &[],
};

static VIBE018: RuleMetadata = RuleMetadata {
    id: "VIBE018",
    module: Module::Vibecheck,
    description: "Highly symmetrical paragraphs",
    default_severity: Severity::Info,
    default_confidence: Confidence::Low,
    message: "Five or more substantial paragraphs have nearly identical lengths.",
    suggestion: "Let paragraph length follow the amount of evidence or explanation each point needs.",
    rationale: "Sustained paragraph-length symmetry is observable but weak evidence of templated expansion, so this rule is informational by default.",
    examples: &["Five 20-plus-word paragraphs whose lengths all fall within a narrow band."],
    false_positives: "Edited publications, slide notes, and constrained layouts may enforce paragraph length intentionally; code comments and steps separated by examples are excluded.",
    replacements: &[],
};

static VIBE019: RuleMetadata = RuleMetadata {
    id: "VIBE019",
    module: Module::Vibecheck,
    description: "Chatbot response residue",
    default_severity: Severity::Warning,
    default_confidence: Confidence::High,
    message: "Repository prose contains an unmistakable chatbot response artifact.",
    suggestion: "Remove the assistant identity or runtime disclaimer and keep only repository-specific information.",
    rationale: "Chatbot identity, cutoff, refusal, browsing, and source-availability disclaimers are interface residue rather than useful project documentation.",
    examples: &["As an AI language model, I do not have access to real-time information."],
    false_positives: "Double-quoted examples are excluded; stored chat transcripts may need a named suppression.",
    replacements: &[],
};

static VIBE020: RuleMetadata = RuleMetadata {
    id: "VIBE020",
    module: Module::Vibecheck,
    description: "Rhetorical contrast formulas",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "The prose repeatedly frames claims as stock rhetorical contrasts.",
    suggestion: "State the claim directly and keep a contrast only where the rejected alternative is one a reader would actually consider.",
    rationale: "Templates such as \"it's X, not Y\", \"the goal isn't X; it's Y\", and the two-sentence \"It's not X. It's Y.\" manufacture a reversal around each claim and make prose predictable without adding information.",
    examples: &[
        "It's a cache, not a database. The goal isn't speed; it's predictability. Rather than tuning the size, measure the misses.",
        "It's not a cache. It's a database. It doesn't guess. It measures.",
    ],
    false_positives: "One or two contrasts never trigger this rule, and double-quoted examples are excluded; comparison guides that weigh named alternatives may still need configuration.",
    replacements: &[],
};

static VIBE021: RuleMetadata = RuleMetadata {
    id: "VIBE021",
    module: Module::Vibecheck,
    description: "Correlative emphasis constructions",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "The prose repeatedly adds emphasis with correlative constructions.",
    suggestion: "State both facts directly; keep a correlative pair only where the second item is genuinely surprising.",
    rationale: "Repeated not-only/but-also, everything-from/to, from-beginners-to-experts, and whether-you're-a constructions add rhetorical lift where a direct sentence would carry the same information.",
    examples: &[
        "Not only is it fast, but it is also clear. It handles everything from scripts to services.",
    ],
    false_positives: "Plain both/and and whether/or clauses never trigger this rule alone, and double-quoted examples are excluded.",
    replacements: &[],
};

static VIBE022: RuleMetadata = RuleMetadata {
    id: "VIBE022",
    module: Module::Vibecheck,
    description: "Workplace productivity jargon",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "Workplace productivity jargon is unusually dense.",
    suggestion: "Name who does what and what changes, instead of leveraging, streamlining, or driving alignment.",
    rationale: "Clusters of empower, leverage, streamline, stakeholder, and alignment language read like an office assistant's summary and hide the concrete action.",
    examples: &[
        "Empower key stakeholders to leverage actionable insights, streamline workflows, and drive alignment across cross-functional teams.",
    ],
    false_positives: "Isolated domain terms such as a best-practices guide or a leveraged buyout do not trigger this rule; management documents may use more of this vocabulary on purpose.",
    replacements: JARGON,
};

static VIBE023: RuleMetadata = RuleMetadata {
    id: "VIBE023",
    module: Module::Vibecheck,
    description: "Generic AI vocabulary density",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "Generic AI-favored vocabulary is unusually dense.",
    suggestion: "Replace abstract evaluation with the specific property, number, or example it stands for.",
    rationale: "Words such as nuanced, thoughtful, meaningful, holistic, and delve are rarely wrong alone, but a high concentration of them signals abstract evaluative padding rather than specific claims.",
    examples: &[
        "A nuanced, holistic review delves into meaningful trade-offs and offers a thoughtful, strategic perspective.",
    ],
    false_positives: "Common technical terms such as context, pattern, constraint, and edge case are not counted; essays about strategy or design may legitimately use several of these words.",
    replacements: AI_VOCABULARY,
};

static VIBE024: RuleMetadata = RuleMetadata {
    id: "VIBE024",
    module: Module::Vibecheck,
    description: "Generalized moral framing",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "The prose turns specific observations into generalized life lessons.",
    suggestion: "End on the specific finding; drop the general lesson unless the source actually argues for it.",
    rationale: "Closers such as \"this is a reminder that\" or \"technology is only as good as the people using it\" add philosophical weight the evidence does not carry.",
    examples: &[
        "This is a reminder that progress isn't always linear. Ultimately, a tool is only as good as the people using it.",
    ],
    false_positives: "One reflective sentence never triggers this rule, and double-quoted examples are excluded; essays and retrospectives may draw lessons on purpose.",
    replacements: MORAL_FRAMING,
};

static VIBE025: RuleMetadata = RuleMetadata {
    id: "VIBE025",
    module: Module::Vibecheck,
    description: "Dramatic characterizations",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "The prose dresses ordinary facts in dramatic characterizations.",
    suggestion: "State the concrete fact, such as the number, date, or change, instead of calling it a pivotal moment or a fundamental shift.",
    rationale: "Phrases such as \"a critical inflection point\" and \"a meaningful step forward\" sound precise but add significance the text does not demonstrate.",
    examples: &[
        "The release marks a pivotal moment and a fundamental shift: a meaningful step forward for the project.",
    ],
    false_positives: "One characterization never triggers this rule, and double-quoted examples are excluded; retrospectives may name a real turning point.",
    replacements: DRAMATIC_CHARACTERIZATIONS,
};

static VIBE026: RuleMetadata = RuleMetadata {
    id: "VIBE026",
    module: Module::Vibecheck,
    description: "Restated question premise",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "The prose restates a reader's question instead of answering it.",
    suggestion: "Start with the answer; the reader already knows what they asked.",
    rationale: "Openers such as \"you're essentially asking whether\" paraphrase a chat prompt and are residue in repository prose, which has no asker.",
    examples: &["You're essentially asking whether the cache can be shared between workers."],
    false_positives: "Double-quoted examples are excluded; FAQ pages written as dialogue may need a named suppression.",
    replacements: &[],
};

static VIBE027: RuleMetadata = RuleMetadata {
    id: "VIBE027",
    module: Module::Vibecheck,
    description: "Chat interface citation artifacts",
    default_severity: Severity::Warning,
    default_confidence: Confidence::High,
    message: "The prose contains a citation marker or tracking parameter left by a chat interface.",
    suggestion: "Replace each marker with the source it stands for, or delete it, and strip chat tracking parameters from links.",
    rationale: "Chat interfaces annotate answers with internal citation markers and tag the links they return; copied into a repository, the markers point nowhere and the parameters only record where the text came from.",
    examples: &[
        "The cache is shared between workers. :contentReference[oaicite:0]{index=0}",
        "See the [guide](https://example.com/guide?utm_source=chatgpt.com) for details.",
    ],
    false_positives: "Double-quoted examples and inline code are excluded; a document about these markers should quote them or put them in code spans.",
    replacements: CITATION_ARTIFACT_REPLACEMENTS,
};

static VIBE028: RuleMetadata = RuleMetadata {
    id: "VIBE028",
    module: Module::Vibecheck,
    description: "Inflated copula substitutes",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "The prose repeatedly replaces a plain is or has with an inflated verb phrase.",
    suggestion: "Write is, are, or has, or say what the subject actually does.",
    rationale: "\"Serves as\", \"stands as\", \"boasts\", and \"plays a pivotal role in\" avoid the plain verb and add weight without adding information.",
    examples: &[
        "The parser serves as the entry point. The cache stands as a key layer. The scheduler plays a pivotal role in throughput.",
    ],
    false_positives: "One or two uses never trigger this rule, and double-quoted examples are excluded; a component that literally acts in a role, such as a proxy, is not counted.",
    replacements: COPULA_SUBSTITUTES,
};

static VIBE029: RuleMetadata = RuleMetadata {
    id: "VIBE029",
    module: Module::Vibecheck,
    description: "Trailing participle commentary",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "Sentences repeatedly end with a participle clause that comments on their own significance.",
    suggestion: "End the sentence at the fact; if the significance matters, state the evidence for it in a sentence of its own.",
    rationale: "Clauses such as \", highlighting its importance\" or \", contributing to a culture of quality\" attach an unsupported interpretation to a fact and read as superficial analysis.",
    examples: &[
        "The team rewrote the parser, highlighting its commitment to quality. Builds now finish sooner, underscoring the value of caching. Reviews moved earlier, fostering collaboration.",
    ],
    false_positives: "Participles that describe an action, such as \", ensuring only one writer holds the lock\", are not counted; fewer than three commentary clauses never trigger this rule, and release notes are excluded.",
    replacements: PARTICIPLE_COMMENTARY,
};

static VIBE030: RuleMetadata = RuleMetadata {
    id: "VIBE030",
    module: Module::Vibecheck,
    description: "Inflated wording density",
    default_severity: Severity::Info,
    default_confidence: Confidence::Medium,
    message: "Long or formal substitutes for plain words are unusually dense.",
    suggestion: "Use the plain word: use for utilize, before for prior to, to for in order to.",
    rationale: "Latinate substitutes such as utilize, thereby, and prior to lengthen sentences without changing their meaning, and corpus comparisons find them far more often in model rewrites than in the human originals.",
    examples: &[
        "In order to ascertain the cause, we utilized the logs prior to the restart, thereby elucidating the failure.",
    ],
    false_positives: "Legal, standards, and academic writing may require formal register; fewer than five uses, or fewer than three distinct forms, never trigger this rule.",
    replacements: PLAIN_WORDS,
};

static VIBE031: RuleMetadata = RuleMetadata {
    id: "VIBE031",
    module: Module::Vibecheck,
    description: "Empty intensifier density",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "Intensifying adverbs are unusually dense.",
    suggestion: "Delete the intensifier, or replace it with the number or comparison it stands for.",
    rationale: "Truly, incredibly, and dramatically assert a degree the sentence does not show; a measurement carries the same emphasis and can be checked.",
    examples: &[
        "The new cache is incredibly fast, truly reliable, and dramatically simpler. It is undeniably an extremely good fit.",
    ],
    false_positives: "Informal writing uses more intensifiers on purpose; fewer than five, or fewer than three distinct forms, never trigger this rule, and double-quoted examples are excluded.",
    replacements: INTENSIFIERS,
};

static VIBE032: RuleMetadata = RuleMetadata {
    id: "VIBE032",
    module: Module::Vibecheck,
    description: "Stock scene-setting opener",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "A paragraph opens with a stock scene-setting phrase.",
    suggestion: "Delete the opener and start with the claim.",
    rationale: "Openers such as \"in today's fast-paced world\" and \"now more than ever\" assert a present-day urgency that every reader already assumes, and delay the first fact.",
    examples: &[
        "In today's fast-paced world, developers need fast feedback.",
        "Now more than ever, teams rely on automation.",
    ],
    false_positives: "One such phrase inside a paragraph is ignored, and double-quoted examples are excluded; historical writing may contrast eras on purpose.",
    replacements: SCENE_SETTERS,
};

static VIBE033: RuleMetadata = RuleMetadata {
    id: "VIBE033",
    module: Module::Vibecheck,
    description: "Unattributed authority",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "The prose cites unnamed studies or experts without a source.",
    suggestion: "Name and link the study, report, or person, or drop the claim.",
    rationale: "\"Studies show\" and \"experts agree\" borrow authority the text cannot be checked against; a named source lets the reader verify the claim.",
    examples: &[
        "Studies show that code review reduces defects. Experts agree that small pull requests are easier to review.",
    ],
    false_positives: "Sentences that carry a link, a numbered or footnote citation, an et al. reference, or a parenthesized year are not counted; one unattributed claim never triggers this rule.",
    replacements: UNATTRIBUTED_AUTHORITY,
};

static VIBE034: RuleMetadata = RuleMetadata {
    id: "VIBE034",
    module: Module::Vibecheck,
    description: "Challenges-and-outlook formula",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "The prose closes on a stock challenges-but-promising outlook.",
    suggestion: "Name the specific open problem and its status, and drop the forecast unless something concrete is planned.",
    rationale: "\"Despite these challenges, it continues to thrive\" and \"only time will tell\" resolve every topic the same way, without naming a problem or a plan.",
    examples: &[
        "Despite these challenges, the project continues to thrive. Only time will tell how it evolves.",
    ],
    false_positives: "One phrase never triggers this rule, and double-quoted examples are excluded; roadmaps that name concrete plans may still use one or two of these phrases.",
    replacements: OUTLOOK_FORMULAS,
};

static VIBE035: RuleMetadata = RuleMetadata {
    id: "VIBE035",
    module: Module::Vibecheck,
    description: "Emoji as structure markers",
    default_severity: Severity::Info,
    default_confidence: Confidence::Low,
    message: "Many headings or list items open with a decorative emoji.",
    suggestion: "Delete the emoji and let the heading or item text carry the meaning.",
    rationale: "Emoji in front of every heading or bullet decorate the structure without adding information and make the document harder to search and read aloud.",
    examples: &["## 🚀 Features\n## 📦 Installation\n## 🛠️ Usage\n## 🤝 Contributing"],
    false_positives: "Status marks such as ✅, ❌, and ⚠️ are not counted, since checklists and compatibility tables use them as data; fewer than four marked lines or three distinct emoji never trigger this rule.",
    replacements: &[],
};

pub(super) fn rules() -> Vec<Box<dyn Rule>> {
    vec![
        boxed(&VIBE002, check_assistant_framing),
        boxed(&VIBE003, check_modifiers),
        boxed(&VIBE004, check_hedging),
        boxed(&VIBE005, check_balance),
        boxed(&VIBE006, check_signposting),
        boxed(&VIBE007, check_em_dashes),
        boxed(&VIBE008, check_colons),
        boxed(&VIBE009, check_openings),
        boxed(&VIBE010, check_rhythm),
        boxed(&VIBE011, check_triads),
        boxed(&VIBE012, check_vague_abstractions),
        boxed(&VIBE013, check_metaphors),
        boxed(&VIBE014, check_corporate_positivity),
        boxed(&VIBE015, check_disclaimers),
        boxed(&VIBE016, check_conclusions),
        boxed(&VIBE017, check_restatement),
        boxed(&VIBE018, check_paragraph_symmetry),
        boxed(&VIBE019, check_chatbot_residue),
        boxed(&VIBE020, check_rhetorical_contrasts),
        boxed(&VIBE021, check_correlatives),
        boxed(&VIBE022, check_workplace_jargon),
        boxed(&VIBE023, check_ai_vocabulary),
        boxed(&VIBE024, check_moral_framing),
        boxed(&VIBE025, check_dramatic_characterizations),
        boxed(&VIBE026, check_restated_premise),
        boxed(&VIBE027, check_citation_artifacts),
        boxed(&VIBE028, check_copula_substitutes),
        boxed(&VIBE029, check_participle_commentary),
        boxed(&VIBE030, check_inflated_wording),
        boxed(&VIBE031, check_intensifiers),
        boxed(&VIBE032, check_scene_setters),
        boxed(&VIBE033, check_unattributed_authority),
        boxed(&VIBE034, check_outlook_formula),
        boxed(&VIBE035, check_emoji_markers),
    ]
}

fn boxed(metadata: &'static RuleMetadata, check_fn: super::CheckFn) -> Box<dyn Rule> {
    Box::new(BuiltinRule { metadata, check_fn })
}

pub(crate) fn is_prose(context: &ScanContext<'_>) -> bool {
    !matches!(
        context.source_type,
        SourceType::Configuration | SourceType::Unknown
    ) && context.prose_word_count() > 0
}

/// Rhythm and symmetry are properties of continuous prose. Comments and docstrings attached to
/// separate declarations are independent texts, so their lengths are not compared.
pub(crate) fn is_running_text(context: &ScanContext<'_>) -> bool {
    matches!(
        context.source_type,
        SourceType::Documentation | SourceType::Text
    ) && context.prose_word_count() > 0
}

/// Release notes repeat one entry template by design, so structural rhythm rules skip them. A
/// project may keep one file per release, as in `CHANGELOG/2026.3.24.md` or
/// `docs/releases/v1.2.0/en.md`, or one fragment per change, as in `changelog.d/1234.bugfix.md`
/// or `release-notes/11457.md`: a release directory, such as `release-notes-published`, with a
/// version or change number below it.
pub(crate) fn is_release_notes(context: &ScanContext<'_>) -> bool {
    let normalize = |name: &str| name.to_ascii_lowercase().replace(['-', '_'], "");
    let stem = context
        .path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(normalize);
    if stem.as_deref().is_some_and(|stem| {
        [
            "changelog",
            "changes",
            "history",
            "news",
            "releases",
            "releasenotes",
        ]
        .contains(&stem)
    }) {
        return true;
    }
    let directories: Vec<_> = context
        .path
        .parent()
        .into_iter()
        .flat_map(|parent| parent.iter())
        .filter_map(|component| component.to_str())
        .map(normalize)
        .collect();
    directories
        .iter()
        .position(|name| {
            ["changelog", "releases", "releasenotes"]
                .iter()
                .any(|known| name.starts_with(known))
        })
        .is_some_and(|index| {
            directories[index + 1..]
                .iter()
                .chain(stem.as_ref())
                .any(|name| name == "unreleased" || is_version(name) || is_change_number(name))
        })
}

/// Whether a lowercase path component names a release, such as `2026.3.24`, `v0.14.1`, or
/// `2026.1.12-1`.
fn is_version(name: &str) -> bool {
    let name = name.strip_prefix('v').unwrap_or(name);
    name.starts_with(|character: char| character.is_ascii_digit())
        && name.contains('.')
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '+'))
}

/// Whether a lowercase path component names a change fragment by its issue or pull request
/// number, such as `11457` or `1234.bugfix`.
fn is_change_number(name: &str) -> bool {
    name.split('.').next().is_some_and(|number| {
        !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
    })
}

pub(crate) fn paragraph_index(context: &ScanContext<'_>, offset: usize) -> Option<usize> {
    let paragraphs = context.paragraphs();
    let index = paragraphs.partition_point(|paragraph| paragraph.end <= offset);
    paragraphs
        .get(index)
        .filter(|paragraph| paragraph.start <= offset)
        .map(|_| index)
}

/// Reports whether `offset` falls on a list item or a table row, or in a paragraph that begins with
/// one. Neighboring list entries and table rows are parallel by design, as in changelogs, option
/// references, and comparison tables, including a list introduced by a lead-in line such as
/// "This skill enforces only:".
pub(crate) fn in_list_paragraph(context: &ScanContext<'_>, offset: usize) -> bool {
    let prose = context.prose();
    let is_entry = |text: &str| {
        let text = text.trim_start();
        is_list_item(text) || text.starts_with('|')
    };
    let line_start = prose[..offset].rfind('\n').map_or(0, |index| index + 1);
    is_entry(&prose[line_start..])
        || paragraph_index(context, offset)
            .is_some_and(|index| is_entry(context.paragraphs()[index].text(prose)))
}

fn check_assistant_framing(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    const PHRASES: &[&str] = &[
        "great question",
        "good question",
        "that's a great point",
        "you're absolutely right",
        "you are absolutely right",
        "here's how",
        "here is how",
        "here's a breakdown",
        "here is a breakdown",
        "here's what you need to know",
        "here is what you need to know",
        "a few things to keep in mind",
        "there are a few things to consider",
        "let's dive",
        "let us dive",
        "let's explore",
        "let us explore",
        "let's break down",
        "let us break down",
        "let's break this down",
        "let's break it down",
        "let's unpack",
        "let's take a closer look",
        "let's take a step back",
        "feel free to",
        "hope this helps",
        "hope that helps",
        "i'd be happy to",
        "i would be happy to",
        "happy to help",
        "i can help with that",
        "i'd recommend",
        "i would recommend",
        "let me know if you'd like",
        "let me know if you want",
        "let me know if you need",
        "let me know if you have any",
        "if you'd like, i can",
        "would you like me to",
        "is there anything else i can",
        "here's a revised",
        "here is a revised",
        "here's an updated version",
        "here is an updated version",
    ];
    /// Interjections count only as a whole sentence, the way an assistant opens a reply.
    const INTERJECTIONS: &[&str] = &["absolutely", "certainly", "of course", "definitely", "sure"];
    if !is_prose(context) {
        return;
    }
    let mut hits = context_phrase_hits(context, PHRASES);
    for sentence in context.sentences() {
        let text = sentence.text(context.prose()).trim();
        let Some(word) = text.strip_suffix(['!', '.']) else {
            continue;
        };
        if let Some(interjection) = INTERJECTIONS
            .iter()
            .find(|interjection| word.eq_ignore_ascii_case(interjection))
            .filter(|_| !inside_double_quotes(context.prose(), sentence.start))
        {
            hits.push((sentence.start, *interjection));
        }
    }
    hits.sort_by_key(|(offset, _)| *offset);
    report_distinct_cluster(
        context,
        metadata,
        findings,
        &hits,
        DistinctThresholds {
            minimum: 2,
            distinct_minimum: 2,
            words_per_hit: 300,
        },
        "assistant-style phrases",
    );
}

const MODIFIERS: &[(&str, &str)] = &[
    ("robust", "name the failure it withstands"),
    ("seamless", "name the step it removes"),
    ("seamlessly", "name the step it removes"),
    ("powerful", "say what it can do"),
    ("comprehensive", "say what it covers"),
    ("effective", "state the measured effect"),
    ("efficient", "give the cost or the speed"),
    ("crucial", "say what breaks without it"),
    ("crucially", "cut it"),
    ("vital", "say what breaks without it"),
    ("important", "say why it matters, or cut it"),
    ("significant", "give the size"),
    ("valuable", "say what it is used for"),
    ("essential", "required, or say what breaks without it"),
    ("critical", "say what breaks without it"),
    ("pivotal", "say what depends on it"),
    ("paramount", "first, or cut it"),
    ("noteworthy", "cut it and state the fact"),
];

fn check_modifiers(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    check_word_density(
        context,
        metadata,
        findings,
        &expressions(MODIFIERS),
        6,
        80,
        "generic modifiers",
    );
}

fn check_hedging(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    const PHRASES: &[&str] = &[
        "perhaps",
        "arguably",
        "generally",
        "typically",
        "often",
        "may",
        "might",
        "could potentially",
        "in many cases",
        "depending on",
        "it seems",
        "tends to",
        "it may be helpful to",
        "it might be worth",
        "it can be argued",
        "it could be said",
        "one could argue",
        "there is a possibility that",
        "it is possible that",
        "generally speaking",
        "to some extent",
        "to a certain extent",
        "in certain situations",
        "in some respects",
        "it is worth considering",
        "doesn't necessarily",
        "does not necessarily",
        "depending on the circumstances",
        "while this may be true",
        "can potentially",
        "may potentially",
        "may be able to",
        "helps ensure",
        "help ensure",
    ];
    check_phrase_cluster(
        context,
        metadata,
        findings,
        PHRASES,
        7,
        70,
        "hedging markers",
    );
}

fn check_balance(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    const STOCK_BALANCE: &[&str] = &[
        "arguments on both sides",
        "both sides have",
        "both approaches have their",
        "each approach has its",
        "have their own strengths",
        "has its own strengths",
        "also has its strengths",
        "advantages and disadvantages",
        "pros and cons",
        "neither approach is inherently",
        "neither option is inherently",
        "neither is inherently better",
        "there is no clear winner",
        "the answer depends on your",
        "depends on your goals",
        "depends on your priorities",
    ];
    if !is_prose(context) {
        return;
    }
    let pairs = context_phrase_hits(context, &["on the one hand", "on the other hand"]);
    let opening = pairs
        .iter()
        .filter(|(_, phrase)| *phrase == "on the one hand")
        .count();
    let paired = opening.min(pairs.len() - opening);
    let stock = context_phrase_hits(context, STOCK_BALANCE);
    let count = paired + stock.len();
    let first = pairs
        .first()
        .into_iter()
        .chain(stock.first())
        .map(|(offset, _)| *offset)
        .min();
    if let Some(offset) = first.filter(|_| count >= 2) {
        emit(
            context,
            metadata,
            findings,
            offset,
            Some(format!("observed {count} artificial balance templates")),
        );
    }
}

fn check_signposting(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    const PHRASES: &[&str] = &[
        "firstly",
        "secondly",
        "thirdly",
        "in this section",
        "as discussed above",
        "as discussed below",
        "to summarize",
        "in conclusion",
        "the following section",
        "next, we",
        "next we",
        "as we have seen",
        "first and foremost",
        "three key points",
        "three key things",
        "three key takeaways",
        "several key points",
        "several key factors",
        "a few key points",
        "a few key things",
        "there are several factors to consider",
        "the first thing to understand",
        "let's look at each",
        "before we dive in",
        "now let's turn to",
        "finally, it's worth noting",
    ];
    check_phrase_cluster(
        context,
        metadata,
        findings,
        PHRASES,
        5,
        100,
        "signposting markers",
    );
}

fn dramatic_colon_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        let pattern = r"(?i)^(?:the (?:[a-z]+ )?(?:result|catch|problem|issue|answer|fix|twist|verdict|takeaway|upshot|lesson|kicker|reason|truth|reality|difference|outcome)(?: is| was)?(?: simple| clear)?|there(?:'s| is| was) (?:one|a|just one|only one) (?:problem|catch|issue|twist|difference|reason)|here(?:'s| is) (?:why|how|the catch|the problem|the thing|what happened|the twist)):\s+\S";
        Regex::new(&pattern.replace('\'', "['\u{2019}]"))
            .expect("VIBE008 reveal regex must compile")
    })
}

/// Offsets of sentences staged as a reveal: a colon after a short dramatic setup ("The result:
/// X.") or a question of at most four words answered in the same paragraph ("What changed? X.").
fn dramatic_reveals(context: &ScanContext<'_>) -> Vec<usize> {
    let prose = context.prose();
    let sentences = context.sentences();
    let mut reveals = Vec::new();
    for (index, sentence) in sentences.iter().enumerate() {
        let text = sentence.text(prose).trim();
        let offset = sentence.start + sentence.text(prose).find(text).unwrap_or(0);
        if inside_double_quotes(prose, offset) {
            continue;
        }
        let answered_question = text.ends_with('?')
            && word_count(text) <= 4
            && sentences.get(index + 1).is_some_and(|next| {
                !next.text(prose).trim_end().ends_with('?')
                    && paragraph_index(context, next.start) == paragraph_index(context, offset)
            });
        if answered_question || dramatic_colon_matcher().is_match(text) {
            reveals.push(offset);
        }
    }
    reveals
}

fn check_em_dashes(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if !is_prose(context) {
        return;
    }
    let offsets: Vec<_> = context
        .prose()
        .match_indices('\u{2014}')
        .map(|(offset, _)| offset)
        .collect();
    if offsets.len() >= 5 && offsets.len() * 100 >= context.prose_word_count() {
        emit(
            context,
            metadata,
            findings,
            offsets[0],
            Some(format!(
                "observed {} em dashes across {} words",
                offsets.len(),
                context.prose_word_count()
            )),
        );
    }
}

/// Whether a sentence uses a colon as punctuation inside running text. A colon needs a space and
/// more text on the same line after it, which leaves out lead-ins that end a line or introduce a
/// list, bold labels such as `**Task:**`, and joined tokens such as `file:line` and URLs.
fn has_prose_colon(text: &str) -> bool {
    text.match_indices(':').any(|(offset, _)| {
        text[offset + 1..]
            .strip_prefix([' ', '\t'])
            .and_then(|rest| rest.split('\n').next())
            .is_some_and(|line| line.chars().any(char::is_alphanumeric))
    })
}

fn check_colons(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if !is_running_text(context) || is_release_notes(context) {
        return;
    }
    let reveals = dramatic_reveals(context);
    if reveals.len() >= 3 && reveals.len() * 200 >= context.prose_word_count() {
        emit(
            context,
            metadata,
            findings,
            reveals[0],
            Some(format!(
                "observed {} dramatic colon or question reveals",
                reveals.len()
            )),
        );
        return;
    }
    let lines: Vec<_> = context.source.lines().collect();
    let sentences: Vec<_> = context
        .sentences()
        .iter()
        .filter(|span| word_count(span.text(context.prose())) >= 3)
        .filter(|span| {
            !in_labeled_record(&lines, context.source[..span.start].matches('\n').count())
        })
        .collect();
    if sentences.len() < 8 {
        return;
    }
    let colon_sentences: Vec<_> = sentences
        .iter()
        .filter(|span| has_prose_colon(span.text(context.prose())))
        .collect();
    if colon_sentences.len() >= 4 && colon_sentences.len() * 5 >= sentences.len() * 2 {
        emit(
            context,
            metadata,
            findings,
            colon_sentences[0].start,
            Some(format!(
                "{} of {} sentences contain colons",
                colon_sentences.len(),
                sentences.len()
            )),
        );
    }
}

fn check_openings(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if !is_prose(context) || is_release_notes(context) || quotes_specification(context) {
        return;
    }
    let lines: Vec<_> = context.source.lines().collect();
    let substantial: Vec<_> = context
        .sentences()
        .iter()
        .filter(|span| {
            !in_labeled_record(&lines, context.source[..span.start].matches('\n').count())
        })
        .filter(|span| {
            !(matches!(context.source_type, SourceType::Code(_))
                && opens_with_code(span.text(context.prose())))
        })
        .filter_map(|span| {
            let tokens = words(span.text(context.prose()));
            (tokens.len() >= 4).then(|| {
                (
                    *span,
                    tokens.into_iter().take(2).collect::<Vec<_>>().join(" "),
                )
            })
        })
        .collect();
    if substantial.len() < 6 {
        return;
    }
    let mut groups: HashMap<(usize, String), Vec<usize>> = HashMap::new();
    let mut scope_counts: HashMap<usize, usize> = HashMap::new();
    for (span, opening) in &substantial {
        let scope = if matches!(context.source_type, SourceType::Code(_)) {
            context
                .paragraphs()
                .iter()
                .position(|paragraph| span.start >= paragraph.start && span.start < paragraph.end)
                .unwrap_or(0)
        } else {
            0
        };
        *scope_counts.entry(scope).or_default() += 1;
        groups
            .entry((scope, opening.clone()))
            .or_default()
            .push(span.start);
    }
    if let Some(((_, opening), offsets)) = groups
        .into_iter()
        .filter(|((scope, _), offsets)| {
            offsets.len() >= 4
                && scope_counts[scope] >= 6
                && offsets.len() * 2 >= scope_counts[scope]
        })
        .max_by(|(left_opening, left), (right_opening, right)| {
            left.len()
                .cmp(&right.len())
                .then_with(|| right[0].cmp(&left[0]))
                .then_with(|| right_opening.cmp(left_opening))
        })
    {
        emit(
            context,
            metadata,
            findings,
            offsets[0],
            Some(format!(
                "{} sentences begin with \"{opening}\"",
                offsets.len()
            )),
        );
    }
}

/// Whether a comment sentence opens with a code expression rather than a word, as in the
/// enumerated examples `1. preStep(Success) -> step1(Running)` or
/// `2. /{:owner}/{:repo}/compare/{:head}`. Such lines show cases; they are not sentences. A
/// bracketed word such as `(optional)` or `[VIA only]` is still prose.
fn opens_with_code(text: &str) -> bool {
    let text = text.trim_start_matches(|character: char| {
        character.is_whitespace()
            || matches!(character, '/' | '#' | '*' | '!' | ';' | '-' | '%' | '<')
    });
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let text = text[digits..].trim_start_matches(['.', ')']).trim_start();
    text.split_whitespace().next().is_some_and(|token| {
        let called = token.find(['(', '[']).is_some_and(|index| {
            token[..index]
                .chars()
                .next_back()
                .is_some_and(|character| character.is_alphanumeric() || character == '_')
        });
        called
            || token.starts_with('/')
            || token.contains(['{', '}', '='])
            || token.contains("->")
            || token.contains("::")
    })
}

/// Whether the prose is the comments of source code that links a web or Internet standard. Such
/// comments quote the specification's algorithm, as in `// If hash is "SHA-384": Set the alg
/// attribute of jwk to "RSA-OAEP-384".`, so its parallel wording is not the author's.
pub(crate) fn quotes_specification(context: &ScanContext<'_>) -> bool {
    matches!(context.source_type, SourceType::Code(_)) && links_specification(context.source)
}

/// Whether a line belongs to a run of three or more consecutive lines that open with the same
/// `Label:` prefix of at most five words, as in pasted log or validator output such as
/// `Validate extension JSON: Error: Field ...`. Each line is a record, not a sentence of prose.
fn in_labeled_record(lines: &[&str], index: usize) -> bool {
    fn label(line: &str) -> Option<&str> {
        let (label, _) = line.trim_start().split_once(": ")?;
        (!label.is_empty()
            && label.split_whitespace().count() <= 5
            && !label.contains(['.', '!', '?']))
        .then_some(label)
    }
    let Some(shared) = lines.get(index).and_then(|line| label(line)) else {
        return false;
    };
    let same = |line: &&&str| label(line) == Some(shared);
    let before = lines[..index].iter().rev().take_while(same).count();
    let after = lines[index + 1..].iter().take_while(same).count();
    before + after >= 2
}

fn check_rhythm(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if !is_running_text(context) || is_release_notes(context) {
        return;
    }
    let runs = prose_runs(context, context.sentences(), 8);
    for window in runs.iter().flat_map(|run| run.windows(8)) {
        let minimum = window.iter().map(|(_, count)| *count).min().unwrap_or(0);
        let maximum = window
            .iter()
            .map(|(_, count)| *count)
            .max()
            .unwrap_or(usize::MAX);
        if maximum.saturating_sub(minimum) <= 3 {
            emit(
                context,
                metadata,
                findings,
                window[0].0.start,
                Some(format!(
                    "eight consecutive sentences range from {minimum} to {maximum} words"
                )),
            );
            return;
        }
    }
}

/// Groups sentences or paragraphs into uninterrupted runs of prose, pairing each span that has at
/// least `minimum_words` with its word count. A code example or a list between two spans ends the
/// run: text on either side belongs to separate steps of a walkthrough, and list entries are
/// parallel by design. Labeled record lines, such as validator output, count as list entries.
fn prose_runs(
    context: &ScanContext<'_>,
    spans: &[Span],
    minimum_words: usize,
) -> Vec<Vec<(Span, usize)>> {
    let lines: Vec<_> = context.source.lines().collect();
    let mut runs = vec![Vec::new()];
    let mut previous_end = 0;
    for span in spans {
        let listed = in_list_paragraph(context, span.start)
            || in_labeled_record(&lines, context.source[..span.start].matches('\n').count());
        let interrupted = context.source[previous_end..span.start]
            .chars()
            .any(char::is_alphanumeric);
        if (listed || interrupted) && runs.last().is_some_and(|run| !run.is_empty()) {
            runs.push(Vec::new());
        }
        previous_end = span.end;
        let count = word_count(span.text(context.prose()));
        if count >= minimum_words && !listed {
            runs.last_mut()
                .expect("at least one run")
                .push((*span, count));
        }
    }
    runs
}

fn check_triads(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if context.source_type != SourceType::Documentation || is_release_notes(context) {
        return;
    }
    let inline = abstract_inline_triads(context);
    if inline.len() >= 3 && inline.len() * 150 >= context.prose_word_count() {
        emit(
            context,
            metadata,
            findings,
            inline[0],
            Some(format!(
                "observed {} inline triads of abstract qualities",
                inline.len()
            )),
        );
        return;
    }
    // Nested items belong to their parent entry, as in an option reference where each option
    // lists its description and an example. A list with nested items is an outline, not a triad.
    let mut groups: Vec<(Vec<usize>, bool)> = Vec::new();
    let mut current = Vec::new();
    let mut outline = false;
    let mut depth = 0;
    let mut nested = 0;
    let mut offset = 0;
    let mut nonblank = 0;
    for line in context.prose().split_inclusive('\n') {
        let trimmed = line.trim_start();
        if !trimmed.trim().is_empty() {
            nonblank += 1;
        }
        if is_list_item(trimmed) {
            let indent = line.len() - trimmed.len();
            if current.is_empty() {
                depth = indent;
            } else if indent > depth {
                nested += 1;
                outline = true;
                offset += line.len();
                continue;
            } else if indent < depth {
                groups.push((std::mem::take(&mut current), std::mem::take(&mut outline)));
                depth = indent;
            }
            current.push(offset);
        } else if !current.is_empty() {
            groups.push((std::mem::take(&mut current), std::mem::take(&mut outline)));
        }
        offset += line.len();
    }
    if !current.is_empty() {
        groups.push((current, outline));
    }
    let triads: Vec<_> = groups
        .iter()
        .filter(|(group, outline)| group.len() == 3 && !outline)
        .map(|(group, _)| group)
        .collect();
    let list_lines = groups.iter().map(|(group, _)| group.len()).sum::<usize>() + nested;
    if triads.len() >= 3 && list_lines * 2 >= nonblank {
        emit(
            context,
            metadata,
            findings,
            triads[0][0],
            Some(format!(
                "observed {} separate three-item list groups",
                triads.len()
            )),
        );
    }
}

/// Offsets of "X, Y, and Z" series whose three members are all single abstract quality words,
/// such as "clear, concise, and compelling" or "speed, reliability, and flexibility". Concrete
/// names and actions, such as "Linux, macOS, and Windows", do not qualify.
fn abstract_inline_triads(context: &ScanContext<'_>) -> Vec<usize> {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    let matcher = MATCHER.get_or_init(|| {
        Regex::new(r"\b([a-z]+), ([a-z]+),? (?:and|or) ([a-z]+)\b")
            .expect("VIBE011 triad regex must compile")
    });
    let prose = context.prose();
    matcher
        .captures_iter(prose)
        .filter(|captures| {
            (1..=3).all(|group| {
                captures
                    .get(group)
                    .is_some_and(|word| is_abstract_quality(word.as_str()))
            })
        })
        .filter_map(|captures| captures.get(0).map(|found| found.start()))
        .filter(|offset| !inside_double_quotes(prose, *offset))
        .collect()
}

fn is_abstract_quality(word: &str) -> bool {
    const QUALITIES: &[&str] = &[
        "clear",
        "concise",
        "simple",
        "fast",
        "safe",
        "clean",
        "easy",
        "lean",
        "smart",
        "strong",
        "people",
        "process",
        "technology",
        "speed",
        "scale",
        "trust",
        "growth",
        "quality",
        "innovation",
        "impact",
        "insight",
        "purpose",
        "clarity",
        "focus",
        "value",
        "compelling",
        "engaging",
    ];
    const SUFFIXES: &[&str] = &[
        "ity", "ness", "ive", "ful", "able", "ible", "ent", "ant", "ous", "ic", "ical", "ance",
        "ence",
    ];
    QUALITIES.contains(&word)
        || (word.len() > 5 && SUFFIXES.iter().any(|suffix| word.ends_with(suffix)))
}

fn is_list_item(line: &str) -> bool {
    if line.starts_with("- ") || line.starts_with("* ") || line.starts_with("+ ") {
        return true;
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && line[digits..].starts_with(". ")
}

fn check_vague_abstractions(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    const PHRASES: &[&str] = &[
        "this approach",
        "that approach",
        "this framework",
        "that framework",
        "this process",
        "that process",
        "this context",
        "that context",
        "this solution",
        "that solution",
        "this strategy",
        "that strategy",
        "this concept",
        "that concept",
        "this system",
        "that system",
        "this aspect",
        "that aspect",
        "this thing",
        "that thing",
        "this dynamic",
        "that dynamic",
        "this distinction",
        "that distinction",
        "this phenomenon",
        "this shift",
        "this reality",
        "this perspective",
        "that perspective",
        "this insight",
        "that insight",
        "this mindset",
        "this tension",
    ];
    check_phrase_cluster(
        context,
        metadata,
        findings,
        PHRASES,
        8,
        80,
        "vague abstraction phrases",
    );
}

const METAPHORS: &[(&str, &str)] = &[
    ("landscape", "field, market, or the specific tools"),
    ("journey", "process, or the steps"),
    ("tapestry", "mix, or list the parts"),
    ("rich tapestry", "mix, or list the parts"),
    ("ecosystem", "the specific tools, packages, or users"),
    ("navigate", "handle, use, or work through"),
    ("unlock", "enable, allow"),
    ("realm", "area, field, or cut it"),
    ("cornerstone", "basis, or say what depends on it"),
    ("bridge the gap", "connect, and name both sides"),
    ("game-changer", "say what changes"),
    ("game changer", "say what changes"),
    ("transformative", "say what changes"),
    ("paradigm", "model, approach"),
    ("tip of the iceberg", "name the rest"),
    ("double-edged sword", "name the benefit and the cost"),
    ("slippery slope", "name the next step and why it follows"),
    ("moving target", "changes often"),
    ("level the playing field", "name who gains access to what"),
    ("connect the dots", "explain the link"),
    ("piece of the puzzle", "part"),
    ("north star", "goal"),
    ("symphony", "combination, or name the parts"),
    ("beacon", "example, or cut it"),
    ("labyrinth", "name the parts that make it hard"),
    ("uncharted territory", "new, untested"),
    ("uncharted waters", "new, untested"),
    ("deep dive", "detailed look, analysis"),
    ("deep-dive", "detailed look, analysis"),
    ("embark on", "start"),
    ("embarks on", "starts"),
    ("the fabric of", "part of"),
    ("treasure trove", "collection, or list it"),
];

fn check_metaphors(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    check_distinct_cluster(
        context,
        metadata,
        findings,
        &expressions(METAPHORS),
        DistinctThresholds {
            minimum: 5,
            distinct_minimum: 4,
            words_per_hit: 120,
        },
        "generic metaphor markers",
    );
}

const POSITIVITY: &[(&str, &str)] = &[
    ("drive success", "name the outcome"),
    ("elevate", "improve, raise"),
    ("elevates", "improves, raises"),
    ("elevating", "improving, raising"),
    ("unlock potential", "say what becomes possible"),
    ("best-in-class", "cite the comparison"),
    ("world-class", "cite the comparison"),
    ("cutting-edge", "new, or name the technique"),
    ("maximize value", "say what improves and by how much"),
    ("accelerate innovation", "say what ships sooner"),
    ("deliver value", "name the benefit"),
    ("exciting opportunity", "opportunity, or say what it allows"),
    ("exciting possibilities", "list them"),
    ("great opportunity to", "a chance to"),
    ("promising development", "state the result"),
    ("powerful tool", "tool, and say what it does"),
    ("transformative potential", "say what could change"),
    ("meaningful impact", "state the effect"),
    ("positive momentum", "state the trend with numbers"),
    ("strong foundation", "name what it supports"),
    ("valuable insights", "state the findings"),
    ("well-positioned to", "able to, or cut it"),
    ("poised to", "about to, or cut it"),
    ("the possibilities are endless", "name two possibilities"),
    ("take it to the next level", "say what improves"),
    ("groundbreaking", "new, or say what it changes"),
    ("revolutionize", "change, and say how"),
    ("revolutionizes", "changes, and say how"),
    ("revolutionizing", "changing, and say how"),
    ("revolutionary", "new, and say what changes"),
    ("unparalleled", "cite the comparison"),
    ("unleash", "use, release"),
    ("supercharge", "speed up, and say by how much"),
];

fn check_corporate_positivity(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    check_distinct_cluster(
        context,
        metadata,
        findings,
        &expressions(POSITIVITY),
        DistinctThresholds {
            minimum: 4,
            distinct_minimum: 3,
            words_per_hit: 150,
        },
        "corporate positivity markers",
    );
}

const JARGON: &[(&str, &str)] = &[
    ("empower", "let, allow"),
    ("empowers", "lets, allows"),
    ("empowering", "letting, allowing"),
    ("leverage", "use"),
    ("leverages", "uses"),
    ("leveraging", "using"),
    ("streamline", "simplify, speed up"),
    ("streamlines", "simplifies, speeds up"),
    ("streamlining", "simplifying, speeding up"),
    ("enhance", "improve"),
    ("enhances", "improves"),
    ("enhancing", "improving"),
    ("foster", "encourage, build"),
    ("fosters", "encourages, builds"),
    ("fostering", "encouraging, building"),
    ("harness", "use"),
    ("harnesses", "uses"),
    ("harnessing", "using"),
    ("facilitate", "help, allow"),
    ("facilitates", "helps, allows"),
    ("facilitating", "helping, allowing"),
    ("spearhead", "lead"),
    ("drive impact", "name the effect"),
    ("drive alignment", "agree on the specific point"),
    ("ensure alignment", "agree on the specific point"),
    ("unlock value", "name the benefit"),
    ("foster collaboration", "say who works together on what"),
    ("foster an environment", "describe the practice"),
    ("enhance productivity", "say what gets faster"),
    ("maximize efficiency", "say what gets cheaper or faster"),
    ("maximize opportunities", "name them"),
    ("optimize workflows", "name the step that changes"),
    ("actionable insights", "findings, recommendations"),
    ("meaningful insights", "findings"),
    ("key stakeholders", "name the people or teams"),
    ("cross-functional", "name the teams"),
    ("move the needle", "change the metric, and give the number"),
    ("low-hanging fruit", "easy fixes, and name them"),
    ("circle back", "return to, follow up on"),
    ("touch base", "talk, meet"),
    ("best practices", "name the practice"),
    ("strategic priorities", "name the goals"),
    ("strategic initiatives", "name the projects"),
    ("business outcomes", "name the result"),
    ("value proposition", "say what it does for whom"),
    ("thought leadership", "name the work, or cut it"),
    ("create synergies", "say what combining them saves"),
    ("synergy", "say what combining them saves"),
    ("end-to-end solution", "say what it covers"),
    ("one-stop shop", "say what it covers"),
    ("turnkey solution", "say what is included"),
    ("address challenges", "name the problem and the fix"),
    ("navigate complexities", "name the problem"),
    ("position ourselves", "prepare to"),
    ("fast-paced world", "cut it"),
];

fn check_workplace_jargon(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    check_distinct_cluster(
        context,
        metadata,
        findings,
        &expressions(JARGON),
        DistinctThresholds {
            minimum: 6,
            distinct_minimum: 4,
            words_per_hit: 100,
        },
        "workplace jargon markers",
    );
}

const AI_VOCABULARY: &[(&str, &str)] = &[
    ("nuanced", "name the distinction"),
    ("nuance", "name the distinction"),
    ("nuances", "name the distinctions"),
    ("thoughtful", "careful, or show the care"),
    ("thoughtfully", "carefully, or show the care"),
    ("meaningful", "say what it changes"),
    ("meaningfully", "say what it changes"),
    ("holistic", "complete, or say what it covers"),
    ("holistically", "say what it covers"),
    ("strategic", "planned, or name the goal"),
    ("strategically", "name the goal"),
    ("compelling", "convincing, or state the reason"),
    ("multifaceted", "name the facets"),
    ("intricate", "complex, or name the parts"),
    ("intricacies", "details"),
    ("intricately", "closely, or say how"),
    ("delve", "look at, examine"),
    ("delves", "looks at, examines"),
    ("delved", "looked at, examined"),
    ("delving", "looking at, examining"),
    ("broader", "wider, or name the scope"),
    ("sustainable", "say how long it lasts"),
    ("profound", "large, or give the size"),
    ("profoundly", "give the size"),
    ("showcase", "show"),
    ("showcases", "shows"),
    ("showcased", "showed"),
    ("showcasing", "showing"),
    (
        "interplay",
        "interaction, or say how they affect each other",
    ),
    ("meticulous", "careful, or show the care"),
    ("meticulously", "carefully, or show the care"),
    ("commendable", "good, and say why"),
    ("garner", "get, earn"),
    ("garnered", "got, earned"),
    ("garners", "gets, earns"),
    ("vibrant", "active, or give the numbers"),
    ("bolster", "strengthen, support"),
    ("bolsters", "strengthens, supports"),
    ("bolstered", "strengthened, supported"),
    ("bolstering", "strengthening, supporting"),
    ("resonate", "appeal to, matter to"),
    ("resonates", "appeals to, matters to"),
    ("unwavering", "steady, or cut it"),
    ("invaluable", "useful, and say for what"),
    ("burgeoning", "growing"),
    ("interconnectedness", "links, or name them"),
    ("impactful", "effective, or state the effect"),
    ("foundational", "basic, or say what depends on it"),
    ("encompass", "cover, include"),
    ("encompasses", "covers, includes"),
    ("encompassing", "covering, including"),
    ("illuminates", "shows, explains"),
    ("unraveling", "untangling, explaining"),
    ("accentuates", "stresses, or cut it"),
];

fn check_ai_vocabulary(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    check_distinct_cluster(
        context,
        metadata,
        findings,
        &expressions(AI_VOCABULARY),
        DistinctThresholds {
            minimum: 5,
            distinct_minimum: 3,
            words_per_hit: 120,
        },
        "generic AI vocabulary words",
    );
}

const MORAL_FRAMING: &[(&str, &str)] = &[
    ("is a reminder that", "cut it and end on the finding"),
    ("serves as a reminder that", "cut it and end on the finding"),
    ("a stark reminder", "cut it and end on the finding"),
    ("reminds us that", "cut it and end on the finding"),
    ("teaches us that", "cut it and end on the finding"),
    ("the lesson is that", "cut it and end on the finding"),
    ("there's a lesson here", "cut it and end on the finding"),
    ("speaks to the importance of", "say why it matters here"),
    ("underscores the importance of", "say why it matters here"),
    ("highlights the importance of", "say why it matters here"),
    ("highlighting the importance of", "say why it matters here"),
    ("speaks volumes", "say what it shows"),
    ("a testament to", "shows, or state what it proves"),
    ("isn't always linear", "describe the actual setbacks"),
    ("is not always linear", "describe the actual setbacks"),
    ("is a journey, not a destination", "cut it"),
    ("is only as good as the", "depends on the"),
    ("is only as effective as the", "depends on the"),
    ("is only as strong as the", "depends on the"),
    ("the human element", "name the people and what they do"),
    ("what matters most", "name it"),
    ("at the end of the day, it's about", "cut it"),
    ("in a world where", "cut it and state the condition"),
];

fn check_moral_framing(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    check_distinct_cluster(
        context,
        metadata,
        findings,
        &expressions(MORAL_FRAMING),
        DistinctThresholds {
            minimum: 2,
            distinct_minimum: 2,
            words_per_hit: 400,
        },
        "generalized lesson phrases",
    );
}

const DRAMATIC_CHARACTERIZATIONS: &[(&str, &str)] = &[
    ("inflection point", "change, with the date or number"),
    ("pivotal moment", "say what changed and when"),
    ("defining moment", "say what changed and when"),
    ("watershed moment", "say what changed and when"),
    ("critical juncture", "decision point, and name the decision"),
    ("turning point", "say what changed"),
    ("fundamental shift", "change, and say what changed"),
    ("seismic shift", "change, and say what changed"),
    ("paradigm shift", "change, and say what changed"),
    ("sea change", "change, and say what changed"),
    ("new era", "say what is new"),
    ("a new chapter", "say what starts"),
    ("transformative opportunity", "name the opportunity"),
    ("powerful framework", "framework, and say what it does"),
    ("compelling case", "state the evidence"),
    ("clear signal", "state what it indicates"),
    ("step forward", "improvement, and give the size"),
    ("game-changing", "say what changes"),
    ("pave the way", "allow, enable"),
    ("paves the way", "allows, enables"),
    ("set the stage for", "allow, enable"),
    ("sets the stage for", "allows, enables"),
    ("indelible mark", "name the lasting effect"),
    ("enduring legacy", "name what remains"),
    ("unprecedented", "first, or give the comparison"),
    ("monumental", "large, and give the size"),
];

fn check_dramatic_characterizations(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    check_distinct_cluster(
        context,
        metadata,
        findings,
        &expressions(DRAMATIC_CHARACTERIZATIONS),
        DistinctThresholds {
            minimum: 2,
            distinct_minimum: 2,
            words_per_hit: 300,
        },
        "dramatic characterizations",
    );
}

const DISCLAIMERS: &[(&str, &str)] = &[
    ("it is important to note", "cut it and state the fact"),
    ("it's important to note", "cut it and state the fact"),
    ("it is worth noting", "cut it and state the fact"),
    ("it's worth noting", "cut it and state the fact"),
    ("it is worth mentioning", "cut it and state the fact"),
    ("it's worth mentioning", "cut it and state the fact"),
    ("it bears mentioning", "cut it and state the fact"),
    ("it should be emphasized", "cut it and state the fact"),
    ("it goes without saying", "cut it"),
    ("needless to say", "cut it"),
    ("keep in mind", "note, or cut it"),
    (
        "results may vary",
        "state the condition that changes the result",
    ),
    ("should be noted", "cut it and state the fact"),
    ("in some cases", "name the cases"),
    ("as with any", "cut it"),
    ("depending on your needs", "name the deciding factor"),
    ("depending on the context", "name the deciding factor"),
    ("one-size-fits-all", "name the cases that differ"),
    (
        "depends on the specific situation",
        "name the deciding factor",
    ),
    ("depends on your specific", "name the deciding factor"),
    ("your individual circumstances", "name the deciding factor"),
    ("your specific circumstances", "name the deciding factor"),
    ("every situation is different", "name what differs"),
    ("not necessarily true in every case", "name the exception"),
];

fn check_disclaimers(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if is_release_notes(context) {
        return;
    }
    check_phrase_cluster(
        context,
        metadata,
        findings,
        &expressions(DISCLAIMERS),
        4,
        0,
        "generic disclaimers",
    );
}

fn check_conclusions(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    const PHRASES: &[&str] = &[
        "in conclusion",
        "to conclude",
        "in summary",
        "to summarize",
        "all in all",
        "at the end of the day",
        "ultimately",
        "in the end",
        "taken together",
        "the key takeaway",
        "the broader lesson",
        "this serves as a reminder",
        "this reinforces the importance",
        "to sum up",
        "in closing",
        "in the final analysis",
        "to wrap up",
    ];
    if !is_prose(context) {
        return;
    }
    let hits = context_phrase_hits(context, PHRASES);
    let latter = hits
        .iter()
        .filter(|(offset, _)| *offset >= context.prose().len() / 2)
        .count();
    if hits.len() >= 3 && latter >= 2 {
        emit(
            context,
            metadata,
            findings,
            hits[0].0,
            Some(format!(
                "observed {} conclusion markers, {latter} in the latter half",
                hits.len()
            )),
        );
    }
}

fn check_restatement(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if !is_prose(context) || is_release_notes(context) || quotes_specification(context) {
        return;
    }
    for pair in context.sentences().windows(2) {
        let paragraph = paragraph_index(context, pair[0].start);
        if paragraph.is_none()
            || paragraph != paragraph_index(context, pair[1].start)
            || in_list_paragraph(context, pair[0].start)
            || on_record_lines(context.source, pair[0].start, pair[1].start)
        {
            continue;
        }
        let first = content_word_set(pair[0].text(context.prose()));
        let second = content_word_set(pair[1].text(context.prose()));
        if first.len() < 7 || second.len() < 7 {
            continue;
        }
        let intersection = first.intersection(&second).count();
        let union = first.union(&second).count();
        if intersection >= 7 && intersection * 4 >= union * 3 {
            emit(
                context,
                metadata,
                findings,
                pair[1].start,
                Some(format!(
                    "adjacent sentence content-word overlap is {intersection}/{union}"
                )),
            );
        }
    }
}

/// Whether two sentences sit on separate lines of a record listing, such as pasted validator
/// output or a run of `/// <param name=...>` docs, where three or more consecutive lines open with
/// the same two words. Each line describes a different item in the same template.
fn on_record_lines(source: &str, first: usize, second: usize) -> bool {
    fn opening(line: &str) -> Option<(&str, &str)> {
        let mut words = line.split_whitespace();
        Some((words.next()?, words.next()?))
    }
    let line_of = |offset: usize| source[..offset].matches('\n').count();
    let (first_line, second_line) = (line_of(first), line_of(second));
    if first_line == second_line {
        return false;
    }
    let lines: Vec<_> = source.lines().collect();
    let Some(shared) = lines.get(first_line).and_then(|line| opening(line)) else {
        return false;
    };
    let same = |index: usize| {
        lines
            .get(index)
            .is_some_and(|line| opening(line) == Some(shared))
    };
    (first_line..=second_line).all(same)
        && (second_line - first_line >= 2
            || first_line.checked_sub(1).is_some_and(same)
            || same(second_line + 1))
}

fn content_word_set(source: &str) -> HashSet<String> {
    const STOP: &[&str] = &[
        "a", "an", "and", "are", "as", "at", "be", "by", "for", "from", "in", "is", "it", "of",
        "on", "or", "that", "the", "this", "to", "was", "with",
    ];
    words(source)
        .into_iter()
        .filter(|word| !STOP.contains(&word.as_str()))
        .collect()
}

fn check_paragraph_symmetry(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if !is_running_text(context) || is_release_notes(context) {
        return;
    }
    let runs = prose_runs(context, context.paragraphs(), 20);
    for window in runs.iter().flat_map(|run| run.windows(5)) {
        let minimum = window.iter().map(|(_, count)| *count).min().unwrap_or(0);
        let maximum = window
            .iter()
            .map(|(_, count)| *count)
            .max()
            .unwrap_or(usize::MAX);
        let tolerance = (minimum / 5).max(4);
        if maximum.saturating_sub(minimum) <= tolerance {
            emit(
                context,
                metadata,
                findings,
                window[0].0.start,
                Some(format!(
                    "five consecutive paragraphs range from {minimum} to {maximum} words"
                )),
            );
            return;
        }
    }
}

fn check_chatbot_residue(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    const PHRASES: &[&str] = &[
        "as an ai language model",
        "as an ai assistant",
        "as of my last knowledge update",
        "my knowledge cutoff",
        "i do not have access to real-time information",
        "i don't have access to real-time information",
        "i cannot browse the internet",
        "i can't browse the internet",
        "as of my last update",
        "as of my latest update",
        "as of my last training",
        "my training cutoff",
        "i'm sorry, but i can't",
        "i'm sorry, but i cannot",
        "i cannot fulfill this request",
        "i can't fulfill this request",
        "i can't assist with that",
        "i cannot assist with that",
        "while specific details are limited",
        "while specific details are scarce",
        "in the provided search results",
        "based on the provided search results",
        "i don't have specific information",
        "i do not have specific information",
    ];
    if !is_prose(context) {
        return;
    }
    let hits = context_phrase_hits(context, PHRASES);
    if let Some((offset, phrase)) = hits.first() {
        emit(
            context,
            metadata,
            findings,
            *offset,
            Some(format!(
                "observed {} chatbot residue phrase(s), beginning with \"{phrase}\"",
                hits.len()
            )),
        );
    }
}

/// Sentence templates that stage a claim as the reversal of a weaker alternative. Each entry is
/// matched case-insensitively against one trimmed sentence; `'` also matches `\u{2019}`.
const CONTRAST_TEMPLATES: &[&str] = &[
    // "It's X, not Y."
    r"^(?:it|this|that)(?:'s| is| was) [^,;.!?]{1,60}, not [^,;.!?]{1,60}[.!]?$",
    // "It's not X; it's Y."
    r"^(?:it|this|that)(?:'s| is| was)(?: not|n't) [^.!?]{1,80}?[,;:\u{2013}\u{2014}] *(?:it|this|that)(?:'s| is| was)\b",
    // "The goal isn't X; it's Y."
    r"^the (?:[a-z-]+ ){0,2}?[a-z-]+ (?:is not|isn't|was not|wasn't) [^.!?]{1,80}?[,;:\u{2013}\u{2014}] *(?:it|this|that)(?:'s| is| was)\b",
    // "Not just X, but Y."
    r"\bnot (?:just|merely) [^.!?]{1,80}?,? but\b",
    // "It's less about X and more about Y."
    r"\bless about [^.!?]{1,80}? (?:and |but )?more about\b",
    // "X isn't the point; Y is."
    r"\b(?:is not|isn't|was not|wasn't) the point\b",
    // "Rather than X, Y."
    r"^rather than [^,.!?]{1,80},",
    // "Instead of asking X, ask Y."
    r"^instead of asking\b",
    // "There's a difference between X and Y."
    r"^there(?:'s| is) an? (?:[a-z]+ )?difference between\b",
    // "The answer isn't necessarily X."
    r"^the answer (?:is not|isn't) necessarily\b",
];

fn contrast_matchers() -> &'static [Regex] {
    static MATCHERS: OnceLock<Vec<Regex>> = OnceLock::new();
    MATCHERS.get_or_init(|| {
        CONTRAST_TEMPLATES
            .iter()
            .map(|template| {
                Regex::new(&format!("(?i){}", template.replace('\'', "['\u{2019}]")))
                    .expect("VIBE020 contrast regexes must compile")
            })
            .collect()
    })
}

/// Matches a contrast split across two sentences, as in "It's not a cache. It's a database.", "It
/// doesn't cache. It caches lazily.", or the short "It doesn't guess. It measures.", and returns
/// its template number after the one-sentence templates. Rust's regex engine has no
/// backreferences, so the repeated verb is compared here.
fn split_contrast(first: &str, second: &str) -> Option<usize> {
    static MATCHERS: OnceLock<[Regex; 3]> = OnceLock::new();
    // Every template opens with it, this, that, or they; skip the regexes for other sentences.
    let opener = |text: &str| {
        text.get(..2).is_some_and(|prefix| {
            prefix.eq_ignore_ascii_case("it") || prefix.eq_ignore_ascii_case("th")
        })
    };
    if !opener(first) || !opener(second) {
        return None;
    }
    let [negated, affirmed, did_not] = MATCHERS.get_or_init(|| {
        let subject = r"(?:it|this|that|they)";
        [
            format!(r"^{subject}(?:(?:'s|'re| is| are| was| were)(?: not| no longer)\b|(?: is| are| was| were)n't\b)"),
            format!(r"^{subject}(?:'s|'re| is| are| was| were)\b"),
            format!(r"^{subject} (?:does|do|did)n't (?:just |only |merely )?([a-z]{{3,}})\b"),
        ]
        .map(|pattern| {
            Regex::new(&format!("(?i){}", pattern.replace('\'', "['\u{2019}]")))
                .expect("VIBE020 split contrast regexes must compile")
        })
    });
    if negated.is_match(first) {
        let rest = &second[affirmed.find(second)?.end()..];
        let rest = rest.to_ascii_lowercase();
        return (!rest.starts_with(" not") && !rest.starts_with("n't"))
            .then_some(CONTRAST_TEMPLATES.len());
    }
    let verb = did_not
        .captures(first)?
        .get(1)?
        .as_str()
        .to_ascii_lowercase();
    let mut words = second.split_whitespace();
    let subject = words.next()?.to_ascii_lowercase();
    let next_word = words
        .next()?
        .trim_matches(|character: char| !character.is_alphabetic())
        .to_ascii_lowercase();
    let same_verb = next_word.starts_with(&verb) && next_word.len() <= verb.len() + 3;
    let aphorism =
        word_count(first) <= SPLIT_APHORISM_WORDS && word_count(second) <= SPLIT_APHORISM_WORDS;
    (matches!(subject.as_str(), "it" | "this" | "that" | "they") && (same_verb || aphorism))
        .then_some(CONTRAST_TEMPLATES.len() + 1)
}

/// The most words in each sentence of a "It doesn't X. It Ys." pair whose verbs differ. A longer
/// second sentence usually explains a failure rather than staging a reversal.
const SPLIT_APHORISM_WORDS: usize = 6;

fn check_rhetorical_contrasts(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if !is_prose(context) || is_release_notes(context) {
        return;
    }
    let prose = context.prose();
    let sentences: Vec<(usize, &str)> = context
        .sentences()
        .iter()
        .map(|sentence| {
            let text = sentence.text(prose);
            // Comment markers and list bullets stay in the prose view; skip them to reach the
            // sentence's first word.
            let trimmed = text.trim_start_matches(|character: char| {
                character.is_whitespace()
                    || matches!(character, '-' | '*' | '+' | '>' | '/' | '#' | '!')
            });
            (
                sentence.start + text.len() - trimmed.len(),
                trimmed.trim_end(),
            )
        })
        .collect();
    let mut hits = Vec::new();
    for (index, (base, trimmed)) in sentences.iter().enumerate() {
        let hit = contrast_matchers()
            .iter()
            .enumerate()
            .find_map(|(template, matcher)| {
                matcher
                    .find(trimmed)
                    .map(|found| (base + found.start(), template))
            })
            .or_else(|| {
                sentences
                    .get(index + 1)
                    .and_then(|(_, next)| split_contrast(trimmed, next))
                    .map(|template| (*base, template))
            });
        if let Some((offset, template)) =
            hit.filter(|(offset, _)| !inside_double_quotes(prose, *offset))
        {
            hits.push((offset, template));
        }
    }
    let distinct: HashSet<_> = hits.iter().map(|(_, template)| *template).collect();
    if hits.len() >= 3 && distinct.len() >= 2 && hits.len() * 250 >= context.prose_word_count() {
        emit(
            context,
            metadata,
            findings,
            hits[0].0,
            Some(format!(
                "observed {} rhetorical contrasts using {} distinct templates across {} words",
                hits.len(),
                distinct.len(),
                context.prose_word_count()
            )),
        );
    }
}

fn correlative_matchers() -> &'static [(Regex, bool)] {
    static MATCHERS: OnceLock<Vec<(Regex, bool)>> = OnceLock::new();
    MATCHERS.get_or_init(|| {
        [
            (r"(?i)\bnot only\b[^.!?]{1,120}?\b(?:but|also)\b", true),
            (
                r"(?i)\b(?:everything|anything) from\b[^.!?]{1,80}?\bto\b",
                true,
            ),
            (
                r"(?i)\bfrom (?:beginners|novices|newcomers|hobbyists|startups|small businesses|individuals) to\b",
                true,
            ),
            (r"(?i)\bwhether you(?:'re|\u{2019}re| are) an? \b", true),
            (r"(?i)\bboth\b[^.!?,;]{1,50}?\band\b", false),
            (r"(?i)\bwhether\b[^.!?;]{1,60}?\bor\b", false),
        ]
        .into_iter()
        .map(|(pattern, strong)| {
            (
                Regex::new(pattern).expect("VIBE021 correlative regexes must compile"),
                strong,
            )
        })
        .collect()
    })
}

/// Strong correlatives ("not only ... but also", "everything from ... to") carry the signal;
/// "both ... and" and "whether ... or" are ordinary grammar and only add weight to a cluster that
/// already has a strong one.
fn check_correlatives(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if !is_prose(context) || is_release_notes(context) {
        return;
    }
    let prose = context.prose();
    let mut strong = Vec::new();
    let mut weak = 0;
    for sentence in context.sentences() {
        let text = sentence.text(prose);
        for (matcher, is_strong) in correlative_matchers() {
            for found in matcher.find_iter(text) {
                let offset = sentence.start + found.start();
                let whether_or_not = found.as_str().eq_ignore_ascii_case("whether or");
                if whether_or_not || inside_double_quotes(prose, offset) {
                    continue;
                }
                if *is_strong {
                    strong.push(offset);
                } else {
                    weak += 1;
                }
            }
        }
    }
    let total = strong.len() + weak;
    let clustered = !strong.is_empty() && total >= 4 && total * 150 >= context.prose_word_count();
    if strong.len() >= 2 || clustered {
        emit(
            context,
            metadata,
            findings,
            strong.iter().copied().min().unwrap_or(0),
            Some(format!(
                "observed {} not-only or everything-from constructions and {weak} both/and or whether/or clauses",
                strong.len()
            )),
        );
    }
}

fn check_restated_premise(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    const PHRASES: &[&str] = &[
        "you're essentially asking",
        "you are essentially asking",
        "what you're really asking",
        "what you are really asking",
        "what you're describing is essentially",
        "the issue you're getting at",
        "the question you're getting at",
        "if i understand your question",
        "it sounds like you're asking",
    ];
    if !is_prose(context) {
        return;
    }
    let hits = context_phrase_hits(context, PHRASES);
    if let Some((offset, phrase)) = hits.first() {
        emit(
            context,
            metadata,
            findings,
            *offset,
            Some(format!("restates the question with \"{phrase}\"")),
        );
    }
}

/// Markers that chat interfaces leave in copied answers, paired with the form a finding names.
/// Each pattern is matched case-insensitively against the prose view.
const CITATION_ARTIFACTS: &[(&str, &str)] = &[
    (r"\boaicite\b", "oaicite"),
    (r"\bcontentreference\[", "contentReference"),
    (r"\boai_citation\b", "oai_citation"),
    (
        r"\bturn\d+(?:search|news|image|view|fetch|file)\d+\b",
        "turn0search0",
    ),
    (r"\battributableindex\b", "attributableIndex"),
    (r"\bgrok_(?:card|render_citation_card_json)\b", "grok_card"),
    (r"\[cite(?:_start|_end|: ?\d+(?:, ?\d+)*)\]", "[cite: 1]"),
    (
        r"\[span_\d+\]\((?:start|end)_span\)",
        "[span_1](start_span)",
    ),
    (
        "\u{3010}\\d+\u{2020}[^\u{3011}]{0,40}\u{3011}",
        "\u{3010}4\u{2020}source\u{3011}",
    ),
    (r"\bppl-ai-file-upload\b", "ppl-ai-file-upload"),
    (r"\[(?:attached_file|web):\d+\]", "[web:1]"),
    (
        r"[?&]utm_source=(?:chatgpt\.com|openai|perplexity(?:\.ai)?|copilot\.com|claude\.ai)\b",
        "utm_source=chatgpt.com",
    ),
    (r":::writing\{", ":::writing"),
];

const CITE_SOURCE: &str = "the source it stands for, as a real link, or delete it";

const CITATION_ARTIFACT_REPLACEMENTS: &[(&str, &str)] = &[
    ("oaicite", CITE_SOURCE),
    ("contentReference", CITE_SOURCE),
    ("oai_citation", CITE_SOURCE),
    ("turn0search0", CITE_SOURCE),
    ("attributableIndex", CITE_SOURCE),
    ("grok_card", CITE_SOURCE),
    ("[cite: 1]", CITE_SOURCE),
    ("[span_1](start_span)", "delete the span markers"),
    ("\u{3010}4\u{2020}source\u{3011}", CITE_SOURCE),
    ("[web:1]", CITE_SOURCE),
    ("ppl-ai-file-upload", "a link to the file, or delete it"),
    (
        "utm_source=chatgpt.com",
        "the URL without the tracking parameter",
    ),
    (":::writing", "delete the wrapper and keep the text"),
];

/// Returns one regex that finds every artifact in a single pass, and the per-artifact regexes that
/// name the form of each match.
fn citation_artifact_matchers() -> &'static (Regex, Vec<(Regex, &'static str)>) {
    static MATCHERS: OnceLock<(Regex, Vec<(Regex, &'static str)>)> = OnceLock::new();
    MATCHERS.get_or_init(|| {
        // ASCII word boundaries keep the regex on its DFA engines; a Unicode `\b` falls back to a
        // slower engine whenever the prose holds a non-ASCII character, and every marker is ASCII.
        let compile = |pattern: &str| {
            Regex::new(&format!("(?i){}", pattern.replace(r"\b", r"(?-u:\b)")))
                .expect("VIBE027 artifact regexes must compile")
        };
        let combined: Vec<String> = CITATION_ARTIFACTS
            .iter()
            .map(|(pattern, _)| format!("(?:{pattern})"))
            .collect();
        (
            compile(&combined.join("|")),
            CITATION_ARTIFACTS
                .iter()
                .map(|(pattern, form)| (compile(pattern), *form))
                .collect(),
        )
    })
}

fn check_citation_artifacts(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if !is_prose(context) {
        return;
    }
    let prose = context.prose();
    let (combined, forms) = citation_artifact_matchers();
    let hits: Vec<(usize, &str)> = combined
        .find_iter(prose)
        .filter(|found| !inside_double_quotes(prose, found.start()))
        .filter_map(|found| {
            forms
                .iter()
                .find(|(matcher, _)| matcher.is_match(found.as_str()))
                .map(|(_, form)| (found.start(), *form))
        })
        .collect();
    let Some((offset, form)) = hits.first() else {
        return;
    };
    emit(
        context,
        metadata,
        findings,
        *offset,
        Some(with_replacements(
            metadata,
            format!(
                "observed {} chat interface artifact(s), beginning with {form}",
                hits.len()
            ),
            hits.iter().map(|(_, form)| *form),
        )),
    );
}

const COPULA_SUBSTITUTES: &[(&str, &str)] = &[
    ("serves as a", "is a"),
    ("serves as an", "is an"),
    ("serves as the", "is the"),
    ("serve as a", "is a, are a"),
    ("serves to", "cut it and keep the verb that follows"),
    ("stands as a", "is a"),
    ("stands as an", "is an"),
    ("stands as the", "is the"),
    ("functions as a", "is a"),
    ("functions as the", "is the"),
    ("boasts a", "has a"),
    ("boasts an", "has an"),
    ("boasts the", "has the"),
    ("boasts over", "has over"),
    ("plays a crucial role in", "say what it does"),
    ("play a crucial role in", "say what they do"),
    ("plays a pivotal role in", "say what it does"),
    ("play a pivotal role in", "say what they do"),
    ("plays a key role in", "say what it does"),
    ("play a key role in", "say what they do"),
    ("plays a vital role in", "say what it does"),
    ("play a vital role in", "say what they do"),
    ("plays a significant role in", "say what it does"),
    ("play a significant role in", "say what they do"),
    ("plays a central role in", "say what it does"),
    ("play a central role in", "say what they do"),
    ("plays a critical role in", "say what it does"),
    ("play a critical role in", "say what they do"),
    ("plays a major role in", "say what it does"),
    ("play a major role in", "say what they do"),
    ("plays an instrumental role in", "say what it does"),
    ("play an instrumental role in", "say what they do"),
    ("plays an important role in", "say what it does"),
    ("play an important role in", "say what they do"),
];

fn check_copula_substitutes(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    check_distinct_cluster(
        context,
        metadata,
        findings,
        &expressions(COPULA_SUBSTITUTES),
        DistinctThresholds {
            minimum: 3,
            distinct_minimum: 2,
            words_per_hit: 200,
        },
        "inflated copula substitutes",
    );
}

const SHOW_EVIDENCE: &str = "end the sentence, then state the evidence, or cut it";

/// Participles that interpret the clause before them. Action participles such as `ensuring` or
/// `making` describe what the code does and are not listed.
const PARTICIPLE_COMMENTARY: &[(&str, &str)] = &[
    ("highlighting", SHOW_EVIDENCE),
    ("underscoring", SHOW_EVIDENCE),
    ("underlining", SHOW_EVIDENCE),
    ("emphasizing", SHOW_EVIDENCE),
    ("emphasising", SHOW_EVIDENCE),
    ("showcasing", SHOW_EVIDENCE),
    ("illustrating", SHOW_EVIDENCE),
    ("demonstrating", SHOW_EVIDENCE),
    (
        "reflecting",
        "name what it reflects and how you know, or cut it",
    ),
    (
        "symbolizing",
        "name what it reflects and how you know, or cut it",
    ),
    (
        "signaling",
        "name what it reflects and how you know, or cut it",
    ),
    (
        "signalling",
        "name what it reflects and how you know, or cut it",
    ),
    ("cementing", "say what changed, or cut it"),
    ("solidifying", "say what changed, or cut it"),
    ("reinforcing", "say what changed, or cut it"),
    ("fostering", "say who does what, or cut it"),
    ("cultivating", "say who does what, or cut it"),
    (
        "contributing to",
        "state the effect and its size, or cut it",
    ),
    ("paving the way", "name what it enables, or cut it"),
    ("setting the stage", "name what it enables, or cut it"),
];

fn participle_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        let forms: Vec<&str> = PARTICIPLE_COMMENTARY
            .iter()
            .map(|(form, _)| *form)
            .collect();
        Regex::new(&format!(
            r"(?i),\s+(?:thereby\s+|further\s+|ultimately\s+)?({})(?-u:\b)",
            forms.join("|")
        ))
        .expect("VIBE029 participle regex must compile")
    })
}

fn check_participle_commentary(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if !is_prose(context) || is_release_notes(context) {
        return;
    }
    let prose = context.prose();
    let hits: Vec<(usize, String)> = participle_matcher()
        .captures_iter(prose)
        .filter_map(|captures| captures.get(1))
        .filter(|found| !inside_double_quotes(prose, found.start()))
        .map(|found| (found.start(), found.as_str().to_ascii_lowercase()))
        .collect();
    let distinct: HashSet<_> = hits.iter().map(|(_, form)| form.as_str()).collect();
    if hits.len() >= 3 && distinct.len() >= 2 && hits.len() * 200 >= context.prose_word_count() {
        emit(
            context,
            metadata,
            findings,
            hits[0].0,
            Some(with_replacements(
                metadata,
                format!(
                    "observed {} trailing commentary clauses using {} distinct participles",
                    hits.len(),
                    distinct.len()
                ),
                hits.iter().map(|(_, form)| form.as_str()),
            )),
        );
    }
}

const PLAIN_WORDS: &[(&str, &str)] = &[
    ("utilize", "use"),
    ("utilizes", "uses"),
    ("utilized", "used"),
    ("utilizing", "using"),
    ("utilization", "use"),
    ("utilise", "use"),
    ("utilised", "used"),
    ("utilising", "using"),
    ("in order to", "to"),
    ("prior to", "before"),
    ("subsequent to", "after"),
    ("in light of", "because of, given"),
    ("with respect to", "about, for"),
    ("with regard to", "about"),
    ("in regard to", "about"),
    ("pertaining to", "about"),
    ("due to the fact that", "because"),
    ("owing to the fact that", "because"),
    ("in the event that", "if"),
    ("at this point in time", "now"),
    ("for the purpose of", "to, for"),
    ("has the ability to", "can"),
    ("in close proximity to", "near"),
    ("in a timely manner", "promptly, or give the deadline"),
    ("a myriad of", "many"),
    ("myriad", "many"),
    ("a plethora of", "many"),
    ("the vast majority of", "most"),
    ("in the realm of", "in"),
    ("akin to", "like"),
    ("commence", "start"),
    ("commences", "starts"),
    ("commenced", "started"),
    ("endeavor", "try, effort"),
    ("endeavors", "tries, efforts"),
    ("endeavour", "try, effort"),
    ("endeavours", "tries, efforts"),
    ("ascertain", "find out, check"),
    ("scrutinize", "examine, check"),
    ("warranting", "justifying, requiring"),
    ("elucidate", "explain"),
    ("elucidates", "explains"),
    ("elucidating", "explaining"),
    ("necessitate", "require"),
    ("necessitates", "requires"),
    ("necessitating", "requiring"),
    ("constitutes", "is"),
    ("delineate", "describe, outline"),
    ("delineates", "describes, outlines"),
    ("aforementioned", "this, the"),
    ("thereby", "so, which"),
    ("henceforth", "from now on"),
];

fn check_inflated_wording(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    check_distinct_cluster(
        context,
        metadata,
        findings,
        &expressions(PLAIN_WORDS),
        DistinctThresholds {
            minimum: 5,
            distinct_minimum: 3,
            words_per_hit: 150,
        },
        "inflated substitutes for plain words",
    );
}

const GIVE_NUMBER: &str = "cut it, or give the number";
const CITE_EVIDENCE: &str = "cut it, or cite the evidence";
const BEFORE_AFTER: &str = "give the before and after numbers";

const INTENSIFIERS: &[(&str, &str)] = &[
    ("truly", "cut it"),
    ("genuinely", "cut it"),
    ("deeply", "cut it"),
    ("utterly", "cut it"),
    ("absolutely", "cut it"),
    ("incredibly", GIVE_NUMBER),
    ("extremely", GIVE_NUMBER),
    ("exceptionally", GIVE_NUMBER),
    ("remarkably", GIVE_NUMBER),
    ("immensely", GIVE_NUMBER),
    ("tremendously", GIVE_NUMBER),
    ("undeniably", CITE_EVIDENCE),
    ("undoubtedly", CITE_EVIDENCE),
    ("unquestionably", CITE_EVIDENCE),
    ("dramatically", BEFORE_AFTER),
    ("drastically", BEFORE_AFTER),
    ("vastly", BEFORE_AFTER),
];

fn check_intensifiers(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    check_distinct_cluster(
        context,
        metadata,
        findings,
        &expressions(INTENSIFIERS),
        DistinctThresholds {
            minimum: 5,
            distinct_minimum: 3,
            words_per_hit: 120,
        },
        "intensifiers",
    );
}

const START_WITH_CLAIM: &str = "cut it and start with the claim";

const SCENE_SETTERS: &[(&str, &str)] = &[
    ("in today's fast-paced", START_WITH_CLAIM),
    ("in today's digital", START_WITH_CLAIM),
    ("in today's rapidly", START_WITH_CLAIM),
    ("in today's ever-changing", START_WITH_CLAIM),
    ("in today's ever-evolving", START_WITH_CLAIM),
    ("in today's modern", START_WITH_CLAIM),
    ("in today's competitive", START_WITH_CLAIM),
    ("in today's interconnected", START_WITH_CLAIM),
    ("in today's world", START_WITH_CLAIM),
    ("in the ever-evolving", START_WITH_CLAIM),
    ("in an ever-evolving", START_WITH_CLAIM),
    ("in the ever-changing", START_WITH_CLAIM),
    ("in an ever-changing", START_WITH_CLAIM),
    ("in the rapidly evolving", START_WITH_CLAIM),
    ("in this day and age", START_WITH_CLAIM),
    ("now more than ever", START_WITH_CLAIM),
    ("in the age of ai", START_WITH_CLAIM),
    ("in the age of artificial intelligence", START_WITH_CLAIM),
    ("in the digital age", START_WITH_CLAIM),
    ("in an era of", START_WITH_CLAIM),
    ("in an era where", START_WITH_CLAIM),
    ("in the modern era", START_WITH_CLAIM),
    ("as technology continues to evolve", START_WITH_CLAIM),
    (
        "imagine a world where",
        "cut it and describe the actual change",
    ),
    ("picture this", START_WITH_CLAIM),
    ("have you ever wondered", "cut it and answer the question"),
    ("ever wondered", "cut it and answer the question"),
    ("look no further", "cut it and say what it does"),
];

/// Reports a scene-setting phrase that opens a paragraph, or two anywhere. Headings, list markers,
/// and comment markers before the phrase still count as its opening.
fn check_scene_setters(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if !is_prose(context) {
        return;
    }
    let prose = context.prose();
    let hits = context_phrase_hits(context, &expressions(SCENE_SETTERS));
    let opens_paragraph = |offset: usize| {
        paragraph_index(context, offset).is_some_and(|index| {
            prose[context.paragraphs()[index].start..offset]
                .chars()
                .all(|character| !character.is_alphanumeric())
        })
    };
    let Some(&(offset, _)) = hits
        .iter()
        .find(|(offset, _)| opens_paragraph(*offset))
        .or_else(|| (hits.len() >= 2).then(|| &hits[0]))
    else {
        return;
    };
    emit(
        context,
        metadata,
        findings,
        offset,
        Some(with_replacements(
            metadata,
            format!("observed {} scene-setting phrase(s)", hits.len()),
            hits.iter().map(|(_, phrase)| *phrase),
        )),
    );
}

const NAME_SOURCE: &str = "name and link the source, or cut the claim";

const UNATTRIBUTED_AUTHORITY: &[(&str, &str)] = &[
    ("studies show", NAME_SOURCE),
    ("studies have shown", NAME_SOURCE),
    ("studies suggest", NAME_SOURCE),
    ("research shows", NAME_SOURCE),
    ("research has shown", NAME_SOURCE),
    ("research suggests", NAME_SOURCE),
    ("experts agree", NAME_SOURCE),
    ("experts say", NAME_SOURCE),
    ("experts argue", NAME_SOURCE),
    ("experts believe", NAME_SOURCE),
    ("experts recommend", NAME_SOURCE),
    ("according to experts", NAME_SOURCE),
    ("many experts", NAME_SOURCE),
    ("industry experts", NAME_SOURCE),
    ("industry reports", NAME_SOURCE),
    ("it is widely accepted", NAME_SOURCE),
    ("it's widely accepted", NAME_SOURCE),
    ("it is widely believed", NAME_SOURCE),
    ("it is generally accepted", NAME_SOURCE),
    ("it is generally understood", NAME_SOURCE),
    ("it's generally understood", NAME_SOURCE),
    ("observers have noted", NAME_SOURCE),
    ("critics argue", NAME_SOURCE),
    ("many people believe", NAME_SOURCE),
    ("the data speaks for itself", "show the data"),
];

/// Matches the citations that attribute a claim: a link, a numbered or footnote reference, an
/// et al. reference, a DOI, or a parenthesized year such as `(Kobak, 2024)`.
fn citation_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(
            r"(?i)https?://|\]\(|\[\^|\[\d+\]|\bet al\b|\bdoi\b|\([^()]*\b(?:1[89]|20)\d\d[a-z]?\)",
        )
        .expect("VIBE033 citation regex must compile")
    })
}

fn check_unattributed_authority(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if !is_prose(context) {
        return;
    }
    let prose = context.prose();
    let sentences = context.sentences();
    let hits: Vec<(usize, &str)> =
        context_phrase_hits(context, &expressions(UNATTRIBUTED_AUTHORITY))
            .into_iter()
            .filter(|(offset, _)| {
                let index = sentences.partition_point(|sentence| sentence.end <= *offset);
                sentences
                    .get(index)
                    .is_none_or(|sentence| !citation_matcher().is_match(sentence.text(prose)))
            })
            .collect();
    if hits.len() >= 2 {
        emit(
            context,
            metadata,
            findings,
            hits[0].0,
            Some(with_replacements(
                metadata,
                format!("observed {} unattributed appeals to authority", hits.len()),
                hits.iter().map(|(_, phrase)| *phrase),
            )),
        );
    }
}

const NAME_PROBLEM: &str = "name the open problem and its status";
const CUT_FORECAST: &str = "cut the forecast, or name the planned change";

const OUTLOOK_FORMULAS: &[(&str, &str)] = &[
    ("despite these challenges", NAME_PROBLEM),
    ("despite its challenges", NAME_PROBLEM),
    ("despite the challenges", NAME_PROBLEM),
    ("faces several challenges", "name the challenges"),
    ("faces numerous challenges", "name the challenges"),
    ("faces significant challenges", "name the challenges"),
    ("challenges and opportunities", "name them"),
    ("continues to thrive", "give the current numbers"),
    (
        "continues to evolve",
        "say what changed recently, or cut it",
    ),
    ("continue to evolve", "say what changed recently, or cut it"),
    ("will continue to shape", CUT_FORECAST),
    ("remains to be seen", "say what would decide it, or cut it"),
    ("only time will tell", CUT_FORECAST),
    ("the future looks bright", CUT_FORECAST),
    ("the future looks promising", CUT_FORECAST),
    ("the future is bright", CUT_FORECAST),
    ("exciting times ahead", CUT_FORECAST),
    ("looking ahead", CUT_FORECAST),
    ("future outlook", CUT_FORECAST),
    ("poised for growth", "give the numbers"),
    ("is only the beginning", CUT_FORECAST),
];

fn check_outlook_formula(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    check_distinct_cluster(
        context,
        metadata,
        findings,
        &expressions(OUTLOOK_FORMULAS),
        DistinctThresholds {
            minimum: 2,
            distinct_minimum: 2,
            words_per_hit: 400,
        },
        "challenges-and-outlook phrases",
    );
}

/// Pictographic ranges whose characters decorate a heading or list item. Status marks such as
/// check marks, crosses, and warning signs record data in checklists and are not counted.
fn is_decorative_emoji(character: char) -> bool {
    const STATUS: &[char] = &[
        '\u{2705}',
        '\u{274C}',
        '\u{274E}',
        '\u{2714}',
        '\u{2716}',
        '\u{2611}',
        '\u{2713}',
        '\u{2717}',
        '\u{2718}',
        '\u{26A0}',
        '\u{2B55}',
        '\u{1F534}',
        '\u{1F7E2}',
        '\u{1F7E1}',
        '\u{1F7E0}',
    ];
    matches!(
        character,
        '\u{1F300}'..='\u{1FAFF}' | '\u{2600}'..='\u{27BF}' | '\u{2B50}' | '\u{23E9}'..='\u{23FA}'
    ) && !STATUS.contains(&character)
}

fn check_emoji_markers(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if context.source_type != SourceType::Documentation {
        return;
    }
    let prose = context.prose();
    let mut marked = Vec::new();
    let mut distinct = HashSet::new();
    let mut offset = 0;
    for line in prose.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let text = if trimmed.starts_with('#') {
            Some(trimmed.trim_start_matches('#'))
        } else if is_list_item(trimmed) {
            trimmed.split_once(' ').map(|(_, rest)| rest)
        } else {
            None
        };
        if let Some(emoji) = text
            .and_then(|text| text.trim_start().chars().next())
            .filter(|character| is_decorative_emoji(*character))
        {
            marked.push(offset + line.len() - trimmed.len());
            distinct.insert(emoji);
        }
        offset += line.len();
    }
    if marked.len() >= 4 && distinct.len() >= 3 {
        emit(
            context,
            metadata,
            findings,
            marked[0],
            Some(format!(
                "observed {} headings or list items that open with an emoji, using {} distinct emoji",
                marked.len(),
                distinct.len()
            )),
        );
    }
}

fn check_word_density(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
    terms: &[&str],
    minimum: usize,
    words_per_hit: usize,
    label: &str,
) {
    if !is_prose(context) {
        return;
    }
    let tokens = words(context.prose());
    let matched: Vec<&str> = tokens
        .iter()
        .map(String::as_str)
        .filter(|word| terms.contains(word))
        .collect();
    let count = matched.len();
    if count >= minimum && count * words_per_hit >= tokens.len() {
        let offset = terms
            .iter()
            .filter_map(|term| find_word(context.lower_prose(), term))
            .min()
            .unwrap_or(0);
        emit(
            context,
            metadata,
            findings,
            offset,
            Some(with_replacements(
                metadata,
                format!("observed {count} {label} across {} words", tokens.len()),
                matched,
            )),
        );
    }
}

fn check_phrase_cluster(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
    phrases: &[&str],
    minimum: usize,
    words_per_hit: usize,
    label: &str,
) {
    if !is_prose(context) {
        return;
    }
    let hits = context_phrase_hits(context, phrases);
    if hits.len() >= minimum
        && (words_per_hit == 0 || hits.len() * words_per_hit >= context.prose_word_count())
    {
        emit(
            context,
            metadata,
            findings,
            hits[0].0,
            Some(with_replacements(
                metadata,
                format!(
                    "observed {} {label} across {} words",
                    hits.len(),
                    context.prose_word_count()
                ),
                hits.iter().map(|(_, phrase)| *phrase),
            )),
        );
    }
}

#[derive(Clone, Copy)]
struct DistinctThresholds {
    minimum: usize,
    distinct_minimum: usize,
    words_per_hit: usize,
}

fn check_distinct_cluster(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
    phrases: &[&str],
    thresholds: DistinctThresholds,
    label: &str,
) {
    if !is_prose(context) {
        return;
    }
    let hits = context_phrase_hits(context, phrases);
    report_distinct_cluster(context, metadata, findings, &hits, thresholds, label);
}

fn report_distinct_cluster(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
    hits: &[(usize, &str)],
    thresholds: DistinctThresholds,
    label: &str,
) {
    let distinct: HashSet<_> = hits.iter().map(|(_, phrase)| *phrase).collect();
    if hits.len() >= thresholds.minimum
        && distinct.len() >= thresholds.distinct_minimum
        && hits.len() * thresholds.words_per_hit >= context.prose_word_count()
    {
        emit(
            context,
            metadata,
            findings,
            hits[0].0,
            Some(with_replacements(
                metadata,
                format!(
                    "observed {} {label} using {} distinct forms",
                    hits.len(),
                    distinct.len()
                ),
                hits.iter().map(|(_, phrase)| *phrase),
            )),
        );
    }
}

/// The most replacements one finding lists; `slopcop explain` prints the full table.
const LISTED_REPLACEMENTS: usize = 6;

/// Appends the replacements for the distinct expressions a finding matched, in the order they first
/// appear, so the observation says what to write instead. Expressions that share a replacement are
/// listed together.
pub(super) fn with_replacements<'a>(
    metadata: &RuleMetadata,
    observation: String,
    matched: impl IntoIterator<Item = &'a str>,
) -> String {
    let mut seen = HashSet::new();
    let mut groups: Vec<(&str, Vec<String>)> = Vec::new();
    for expression in matched {
        let expression = expression.trim();
        let Some(replacement) = metadata.replacement_for(expression) else {
            continue;
        };
        if !seen.insert(expression.to_lowercase()) {
            continue;
        }
        let quoted = format!("\"{expression}\"");
        if let Some((_, expressions)) = groups.iter_mut().find(|(shared, _)| *shared == replacement)
        {
            expressions.push(quoted);
        } else if groups.len() < LISTED_REPLACEMENTS {
            groups.push((replacement, vec![quoted]));
        }
    }
    if groups.is_empty() {
        return observation;
    }
    let listed: Vec<String> = groups
        .iter()
        .map(|(replacement, expressions)| format!("{} -> {replacement}", expressions.join(", ")))
        .collect();
    format!("{observation}; replacements: {}", listed.join("; "))
}

/// Lists the expressions of a replacement table, for rules whose table is also their phrase list.
fn expressions(table: &[(&'static str, &'static str)]) -> Vec<&'static str> {
    table.iter().map(|(expression, _)| *expression).collect()
}

/// Finds whole-phrase occurrences in lowercase prose. A phrase written with `'` also matches a
/// typographic apostrophe, and a phrase nested inside a longer match, such as `may` inside
/// `it may be helpful to`, is not counted again.
fn phrase_hits<'a>(haystack: &str, phrases: &[&'a str]) -> Vec<(usize, &'a str)> {
    let mut hits = Vec::new();
    with_phrase_matcher(phrases, |matcher, owners| {
        // Each spelling advances past its own previous match, as repeated `str::find` calls do, so
        // a spelling never overlaps itself; different phrases may overlap and are resolved below.
        // One spelling has one length, so its matches arrive in start order.
        let mut resume = vec![0; owners.len()];
        for found in matcher.find_overlapping_iter(haystack) {
            let pattern = found.pattern().as_usize();
            if found.start() < resume[pattern] {
                continue;
            }
            resume[pattern] = found.end();
            let before = haystack[..found.start()].chars().next_back();
            let after = haystack[found.end()..].chars().next();
            let boundary = |character: Option<char>| {
                character.is_none_or(|value| !value.is_alphanumeric() && value != '_')
            };
            if boundary(before) && boundary(after) {
                hits.push((found.start(), found.end(), phrases[owners[pattern]]));
            }
        }
    });
    hits.sort_by_key(|(offset, end, _)| (*offset, std::cmp::Reverse(*end)));
    let mut covered = 0;
    hits.into_iter()
        .filter_map(|(offset, end, phrase)| {
            if end <= covered {
                return None;
            }
            covered = covered.max(end);
            Some((offset, phrase))
        })
        .collect()
}

/// Runs `search` with one automaton over every spelling of `phrases`, plain and with a typographic
/// apostrophe, and the index of the phrase each spelling belongs to. Rules pass the same few phrase
/// lists for every file, so each worker thread builds an automaton once per list. The cache is
/// keyed by content rather than address because callers may pass temporary arrays.
fn with_phrase_matcher<R>(phrases: &[&str], search: impl FnOnce(&AhoCorasick, &[usize]) -> R) -> R {
    type Matchers = HashMap<Vec<String>, (AhoCorasick, Vec<usize>)>;
    thread_local! {
        static MATCHERS: RefCell<Matchers> = RefCell::new(HashMap::new());
    }
    MATCHERS.with(|matchers| {
        let mut matchers = matchers.borrow_mut();
        let key: Vec<String> = phrases.iter().map(|phrase| (*phrase).to_owned()).collect();
        let (matcher, owners) = matchers.entry(key).or_insert_with(|| {
            let mut spellings = Vec::new();
            let mut owners = Vec::new();
            for (index, phrase) in phrases.iter().enumerate() {
                let typographic = phrase.replace('\'', "\u{2019}");
                if typographic != *phrase {
                    spellings.push(typographic);
                    owners.push(index);
                }
                spellings.push((*phrase).to_owned());
                owners.push(index);
            }
            let matcher = AhoCorasick::new(&spellings).expect("phrase automaton must build");
            (matcher, owners)
        });
        search(matcher, owners)
    })
}

fn context_phrase_hits<'a>(
    context: &ScanContext<'_>,
    phrases: &[&'a str],
) -> Vec<(usize, &'a str)> {
    phrase_hits(context.lower_prose(), phrases)
        .into_iter()
        .filter(|(offset, _)| !inside_double_quotes(context.prose(), *offset))
        .collect()
}

pub(super) fn inside_double_quotes(source: &str, offset: usize) -> bool {
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let prefix = &source[line_start..offset];
    let mut quoted = false;
    for character in prefix.chars() {
        if character == '"' {
            quoted = !quoted;
        }
    }
    quoted || prefix.rfind('“') > prefix.rfind('”')
}

fn find_word(haystack: &str, needle: &str) -> Option<usize> {
    phrase_hits(haystack, &[needle])
        .first()
        .map(|(offset, _)| *offset)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn findings(rule_id: &str, source: &str) -> usize {
        findings_at(rule_id, "README.md", source)
    }

    fn findings_at(rule_id: &str, path: &str, source: &str) -> usize {
        let path = Path::new(path);
        let context = ScanContext::new(path, source, crate::language::classify(path));
        let rule = rules()
            .into_iter()
            .find(|rule| rule.metadata().id == rule_id)
            .expect("registered rule");
        let mut results = Vec::new();
        rule.check(&context, &mut results);
        results.len()
    }

    #[test]
    fn density_rules_require_clusters() {
        let cases = [
            (
                "VIBE002",
                "Great question. Let's dive into it. Feel free to ask for more.",
                "The parser reads one token at a time.",
            ),
            (
                "VIBE003",
                "This robust, seamless, powerful, comprehensive, crucial, and effective tool works.",
                "This robust parser rejects malformed input.",
            ),
            (
                "VIBE004",
                "Perhaps it may generally work. It might often help, arguably, and could potentially fit in many cases.",
                "The cache may be stale after a network partition.",
            ),
            (
                "VIBE005",
                "There are valid arguments on both sides. Both approaches have their advantages and disadvantages.",
                "The binary is small, but startup time matters more.",
            ),
            (
                "VIBE006",
                "Firstly, inspect it. Secondly, test it. Thirdly, ship it. In this section, we proceed. To summarize, it works.",
                "First, parse the header. Then validate its checksum.",
            ),
            (
                "VIBE012",
                "This approach improves that process. This framework supports that context. This solution fits that strategy. This concept shapes that system.",
                "This approach uses a radix tree; lookup remains logarithmic.",
            ),
            (
                "VIBE013",
                "Navigate the landscape and unlock the ecosystem. This transformative journey crosses a new paradigm and bridges the gap.",
                "The deployment landscape contains three regions.",
            ),
            (
                "VIBE014",
                "This exciting opportunity builds a strong foundation, delivers valuable insights, and helps us drive success.",
                "Profiling is a powerful tool for finding hot loops.",
            ),
            (
                "VIBE015",
                "It is important to note the limit. Keep in mind the cost. Results may vary. As with any solution, measure it.",
                "Keep in mind that this endpoint deletes data.",
            ),
            (
                "VIBE016",
                "In summary, measurements decide. The benchmark records latency, throughput, allocation count, startup time, and peak memory before any decision is made. Ultimately, the tests pass. In conclusion, ship it.",
                "In conclusion, the measured p95 stayed below 10 ms.",
            ),
            (
                "VIBE028",
                "The parser serves as the entry point. The cache stands as a key layer. The scheduler plays a pivotal role in throughput.",
                "The parser serves as the entry point. The proxy acts as a gateway, and the cache acts as a buffer.",
            ),
            (
                "VIBE030",
                "In order to ascertain the cause, we utilized the logs prior to the restart, thereby isolating the failure.",
                "Logs written prior to version 2 use the old format, so the reader converts them first.",
            ),
            (
                "VIBE031",
                "It is incredibly fast, truly reliable, and dramatically simpler. It is undeniably an extremely good fit.",
                "It answers in 2 ms instead of 40 ms, and it is truly optional.",
            ),
            (
                "VIBE034",
                "Despite these challenges, the project continues to thrive. Only time will tell.",
                "Looking ahead, the 2.0 release removes the legacy loader.",
            ),
        ];
        for (rule, bad, clean) in cases {
            assert_eq!(findings(rule, bad), 1, "{rule} should trigger");
            assert_eq!(findings(rule, clean), 0, "{rule} should stay quiet");
        }
    }

    fn observation(rule_id: &str, source: &str) -> String {
        let path = Path::new("README.md");
        let context = ScanContext::new(path, source, crate::language::classify(path));
        let rule = rules()
            .into_iter()
            .find(|rule| rule.metadata().id == rule_id)
            .expect("registered rule");
        let mut results = Vec::new();
        rule.check(&context, &mut results);
        results
            .pop()
            .and_then(|finding| finding.observation)
            .unwrap_or_default()
    }

    #[test]
    fn findings_name_replacements_for_matched_expressions() {
        let observed = observation(
            "VIBE030",
            "In order to ascertain the cause, we utilized the logs prior to the restart, thereby isolating the failure.",
        );
        assert!(
            observed.ends_with(
                r#"replacements: "in order to" -> to; "ascertain" -> find out, check; "utilized" -> used; "prior to" -> before; "thereby" -> so, which"#
            ),
            "{observed}"
        );
        let grouped = observation(
            "VIBE027",
            "Shared. :contentReference[oaicite:0]{index=0} See [the guide](https://example.com/?utm_source=chatgpt.com).",
        );
        assert!(
            grouped.contains(
                r#"replacements: "contentReference", "oaicite" -> the source it stands for"#
            ),
            "{grouped}"
        );
    }

    #[test]
    fn every_listed_expression_has_a_replacement_and_no_duplicates() {
        for rule in rules() {
            let metadata = rule.metadata();
            let mut seen = HashSet::new();
            for (expression, replacement) in metadata.replacements {
                assert!(!replacement.is_empty(), "{} {expression}", metadata.id);
                assert!(
                    seen.insert(*expression),
                    "{} lists {expression} twice",
                    metadata.id
                );
                assert_eq!(
                    metadata.replacement_for(expression),
                    Some(*replacement),
                    "{} {expression}",
                    metadata.id
                );
            }
        }
    }

    #[test]
    fn citation_artifacts_trigger_alone_but_not_in_code_or_quotes() {
        for artifact in [
            "Shared between workers.:contentReference[oaicite:3]{index=3}",
            "See the notes turn0search12 for details.",
            "The parser is fast [cite: 4, 12].",
            "The parser is fast.\u{3010}12\u{2020}L3-L9\u{3011}",
            "Read [the guide](https://example.com/guide?utm_source=chatgpt.com).",
            "[span_1](start_span)The parser is fast.[span_1](end_span)",
        ] {
            assert_eq!(findings("VIBE027", artifact), 1, "{artifact}");
        }
        assert_eq!(
            findings(
                "VIBE027",
                "Strip `oaicite` markers and the \"utm_source=chatgpt.com\" parameter."
            ),
            0
        );
        assert_eq!(
            findings_at(
                "VIBE027",
                "clean.py",
                "PATTERN = \"contentReference[oaicite:0]\"\n"
            ),
            0
        );
    }

    #[test]
    fn participle_commentary_ignores_action_participles() {
        assert_eq!(
            findings(
                "VIBE029",
                "The team rewrote the parser, highlighting its commitment to quality. Builds finish sooner, underscoring the value of caching. Reviews moved earlier, fostering collaboration."
            ),
            1
        );
        assert_eq!(
            findings(
                "VIBE029",
                "The writer takes the lock, ensuring only one process appends. The reader copies the buffer, making later writes safe. The cache evicts old keys, keeping memory flat."
            ),
            0
        );
        assert_eq!(
            findings(
                "VIBE029",
                "The team rewrote the parser, highlighting its commitment to quality. Builds finish sooner, highlighting the value of caching."
            ),
            0
        );
    }

    #[test]
    fn scene_setters_trigger_when_they_open_a_paragraph() {
        assert_eq!(
            findings(
                "VIBE032",
                "In today's fast-paced world, feedback must be fast."
            ),
            1
        );
        assert_eq!(
            findings(
                "VIBE032",
                "## Now more than ever\n\nThe linter runs in a second."
            ),
            1
        );
        assert_eq!(
            findings(
                "VIBE032",
                "Programmers who worked in the digital age of punch cards waited a day."
            ),
            0
        );
        assert_eq!(
            findings(
                "VIBE032",
                "Programmers in the digital age had punch cards. They waited, now more than ever."
            ),
            1
        );
    }

    #[test]
    fn unattributed_authority_accepts_cited_sentences() {
        assert_eq!(
            findings(
                "VIBE033",
                "Studies show that review reduces defects. Experts agree that small changes are easier."
            ),
            1
        );
        assert_eq!(
            findings(
                "VIBE033",
                "Studies show that review reduces defects (Bacchelli and Bird, 2013). Experts agree, as the [survey](https://example.com) reports."
            ),
            0
        );
        assert_eq!(
            findings(
                "VIBE033",
                "Studies show that review reduces defects [3]. Experts agree that small changes are easier."
            ),
            0
        );
    }

    #[test]
    fn emoji_markers_skip_status_marks() {
        assert_eq!(
            findings(
                "VIBE035",
                "## \u{1F680} Features\n\n## \u{1F4E6} Install\n\n## \u{1F6E0}\u{FE0F} Usage\n\n- \u{1F91D} Contributing\n"
            ),
            1
        );
        assert_eq!(
            findings(
                "VIBE035",
                "- \u{2705} Linux\n- \u{2705} macOS\n- \u{274C} Windows\n- \u{26A0}\u{FE0F} FreeBSD\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "VIBE035",
                "## \u{1F680} Features\n\n## \u{1F680} Install\n\n## \u{1F680} Usage\n\n## \u{1F680} Help\n"
            ),
            0
        );
    }

    #[test]
    fn contrasts_split_across_sentences_count() {
        assert_eq!(
            findings(
                "VIBE020",
                "It's not a cache. It's a database. It doesn't guess. It measures. The goal isn't speed; it's predictability."
            ),
            1
        );
        assert_eq!(
            findings(
                "VIBE020",
                "It's not a cache. It's not a database either. It doesn't guess. Then it reads the log."
            ),
            0
        );
        assert_eq!(
            split_contrast("This isn't a bug.", "It is expected."),
            Some(CONTRAST_TEMPLATES.len())
        );
        assert_eq!(
            split_contrast("It doesn't guess.", "It measures."),
            Some(CONTRAST_TEMPLATES.len() + 1)
        );
        assert_eq!(
            split_contrast(
                "It doesn't cache.",
                "It caches lazily after the first read."
            ),
            Some(CONTRAST_TEMPLATES.len() + 1)
        );
        assert_eq!(
            split_contrast(
                "It doesn't compile.",
                "It fails with error E0502 on line 4."
            ),
            None
        );
    }

    #[test]
    fn workplace_jargon_counts_inflected_forms() {
        assert_eq!(
            findings(
                "VIBE022",
                "Leveraging shared tooling empowers teams, streamlining reviews and fostering ownership while enhancing velocity and harnessing feedback."
            ),
            1
        );
    }

    #[test]
    fn reference_phrases_extend_existing_rules() {
        assert_eq!(
            findings(
                "VIBE019",
                "I'm sorry, but I can't share the deployment keys."
            ),
            1
        );
        assert_eq!(
            findings(
                "VIBE021",
                "Whether you're a beginner or an expert, it helps. It suits everyone from startups to enterprises."
            ),
            1
        );
        assert_eq!(
            findings(
                "VIBE023",
                "The release showcases a vibrant community, bolstered by meticulous reviews and an invaluable interplay of ideas."
            ),
            1
        );
    }

    #[test]
    fn quoted_phrase_examples_do_not_count_as_author_voice() {
        let assistant_examples =
            r#"Say \"That's a great question.\" Then say \"I would be happy to help.\""#;
        assert_eq!(findings("VIBE002", assistant_examples), 0);
        assert_eq!(
            findings(
                "VIBE002",
                "Great question. I would be happy to walk through the answer."
            ),
            1
        );

        let conclusion_examples =
            r#"Avoid \"In conclusion\", \"In summary\", and \"Ultimately\" in one passage."#;
        assert_eq!(findings("VIBE016", conclusion_examples), 0);
    }

    #[test]
    fn standard_api_sections_are_not_one_repeated_prose_passage() {
        let source = "/// Writes an output report.\n///\n/// # Errors\n/// Returns the underlying writer error.\nfn write() { output(); }\n\n".repeat(8);
        let context = ScanContext::new(
            Path::new("report.rs"),
            &source,
            SourceType::Code(crate::language::Language::Rust),
        );
        let mut results = Vec::new();
        check_openings(&context, &VIBE009, &mut results);
        assert_eq!(results, [] as [crate::model::Finding; 0]);
    }

    #[test]
    fn tied_sentence_openings_choose_the_first_source_group() {
        let source = format!(
            "{}{}",
            "This system reads source files. ".repeat(4),
            "That service writes output records. ".repeat(4)
        );
        let context = ScanContext::new(Path::new("README.md"), &source, SourceType::Documentation);
        for _ in 0..20 {
            let mut results = Vec::new();
            check_openings(&context, &VIBE009, &mut results);
            assert_eq!(results[0].location.line, 1);
            assert_eq!(results[0].location.column, 1);
        }
    }

    #[test]
    fn structural_rules_measure_repetition() {
        let em_dashes = "One claim \u{2014} one caveat \u{2014} one result \u{2014} one limit \u{2014} one decision \u{2014} one action.";
        assert_eq!(findings("VIBE007", em_dashes), 1);
        assert_eq!(
            findings("VIBE007", "One claim \u{2014} one measured caveat."),
            0
        );

        let colons = "Input: read bytes. Output: return tokens. Limit: reject overflow. Cost: one allocation. The parser is deterministic. The scanner is parallel. Errors name the path. Tests cover failures.";
        assert_eq!(findings("VIBE008", colons), 1);

        let openings = "This system reads source files. This system classifies each source. This system runs every rule. This system sorts every finding. Other reporters consume the result. Tests verify stable output.";
        assert_eq!(findings("VIBE009", openings), 1);

        let rhythm = "Workers read each source file exactly one time. Rules inspect cached prose without repeated allocation overhead. Findings retain exact source offsets for stable reporting. Reporters sort every result before emitting deterministic output. Configuration changes severity without mutating detector rule logic. Git modes restrict scans to currently relevant files. Tests cover positive and negative fixture behavior carefully. Benchmarks measure complete repository scans under realistic loads.";
        assert_eq!(findings("VIBE010", rhythm), 1);

        let triads =
            "- alpha\n- beta\n- gamma\n\n- delta\n- epsilon\n- zeta\n\n- eta\n- theta\n- iota\n";
        assert_eq!(findings("VIBE011", triads), 1);
    }

    #[test]
    fn comments_quoting_a_linked_specification_are_not_authored_prose() {
        let quoted = include_str!("../../tests/fixtures/clean/spec-quotes.rs");
        assert_eq!(findings_at("VIBE017", "sha_operation.rs", quoted), 0);
        assert_eq!(findings_at("VIBE009", "sha_operation.rs", quoted), 0);
        let unlinked = quoted.replacen(
            "<https://w3c.github.io/webcrypto/#sha-operations-digest>",
            "Computes a digest.",
            1,
        );
        assert!(findings_at("VIBE017", "sha_operation.rs", &unlinked) > 0);
        assert_eq!(findings_at("VIBE009", "sha_operation.rs", &unlinked), 1);
    }

    #[test]
    fn labeled_record_lines_are_not_prose_sentences() {
        let output = "Validate extension JSON: Error: Field 'classes/A/methods/a': size changed value in new API, from 9 to 10.\nValidate extension JSON: Error: Field 'classes/B/methods/b': size changed value in new API, from 10 to 7.\nValidate extension JSON: Error: Field 'classes/C/methods/c': type changed value in new API, from int to float.\nValidate extension JSON: Error: Field 'classes/D/methods/d': meta changed value in new API, from float to int.\nValidate extension JSON: Error: Field 'classes/E/methods/e': default_value changed value in new API, from 1 to 0.\nValidate extension JSON: Error: Field 'classes/F/methods/f': size changed value in new API, from 3 to 4.\n";
        assert_eq!(findings_at("VIBE009", "GH-98670.txt", output), 0);
        let anaphora = "This system reads source files.\nThis system classifies each source.\nThis system runs every rule.\nThis system sorts every finding.\nOther reporters consume the result.\nTests verify stable output.\n";
        assert_eq!(findings("VIBE009", anaphora), 1);
        assert_eq!(findings_at("VIBE008", "GH-98670.txt", output), 0);
        let uniform = "Validate extension JSON: Error: Field 'classes/A/methods/a': size changed value in new API, from 9 to 10.\n".repeat(9);
        assert_eq!(findings_at("VIBE010", "GH-98670.txt", &uniform), 0);
        let labels = "Note: this system reads source files.\nNote: this system classifies each source.\nThis system runs every rule.\nThis system sorts every finding.\nThis system writes every report.\nThis system exits cleanly.\n";
        assert_eq!(findings("VIBE009", labels), 1);
    }

    #[test]
    fn record_lines_are_entries_not_restatements() {
        let output = "Validate extension JSON: Error: Field 'classes/Control/methods/get_theme_font/arguments/1': default_value changed value in new API, from none to empty.\nValidate extension JSON: Error: Field 'classes/Control/methods/get_theme_icon/arguments/1': default_value changed value in new API, from none to empty.\nValidate extension JSON: Error: Field 'classes/Control/methods/get_theme_color/arguments/1': default_value changed value in new API, from none to empty.\n";
        assert_eq!(findings_at("VIBE017", "GH-84906.txt", output), 0);
        let params = "/// <param name=\"albumArtistIds\">Optional. If specified, results will be filtered to include only those containing the specified album artist id.</param>\n/// <param name=\"contributingArtistIds\">Optional. If specified, results will be filtered to include only those containing the specified contributing artist id.</param>\n/// <param name=\"albums\">Optional. If specified, results will be filtered based on album.</param>\npublic void Get() {}\n";
        assert_eq!(findings_at("VIBE017", "ItemsController.cs", params), 0);
        let two_lines = "The scanner reads every tracked source file in parallel during repository checks.\nThe scanner reads every tracked source file in parallel during each repository check.\n";
        assert_eq!(findings("VIBE017", two_lines), 1);
    }

    #[test]
    fn local_restatements_report_separate_occurrences() {
        let source = "The scanner reads every tracked source file in parallel during repository checks. Every tracked source file is read in parallel by the scanner during repository checks. A different subject separates independent passages here. The parser validates every incoming network packet before dispatching requests. Every incoming network packet is validated before dispatching requests by the parser.";
        assert_eq!(findings("VIBE017", source), 2);
    }

    #[test]
    fn similarity_and_symmetry_rules_have_negative_controls() {
        let repeated = "The scanner reads every tracked source file in parallel during repository checks. Every tracked source file is read in parallel by the scanner during repository checks.";
        assert_eq!(findings("VIBE017", repeated), 1);
        assert_eq!(
            findings(
                "VIBE017",
                "The scanner reads files in parallel. Reporters sort findings by location."
            ),
            0
        );

        let paragraph = "Each paragraph contains exactly enough ordinary words to cross the substantial length threshold while describing one measured behavior clearly today.";
        let symmetrical = std::iter::repeat_n(paragraph, 5)
            .collect::<Vec<_>>()
            .join("\n\n");
        assert_eq!(findings("VIBE018", &symmetrical), 1);
        assert_eq!(
            findings("VIBE018", "A short note.\n\nA second short note."),
            0
        );
    }

    #[test]
    fn sparse_tutorial_phrases_and_nouns_are_not_clusters() {
        let filler =
            "The search walks each directory and skips ignored files before reading. ".repeat(60);
        let tutorial = format!(
            "Here's how to search. {filler} Here's how to filter. {filler} Here is how to sort."
        );
        assert_eq!(findings("VIBE002", &tutorial), 0);
        let keys = "Send the API key in the header. Each key belongs to one project. A revoked key fails. Rotate the key from the dashboard. The old key stays valid for an hour. Workers read the new key.";
        assert_eq!(findings("VIBE003", keys), 0);
    }

    #[test]
    fn colon_patterns_ignore_lead_ins_code_and_release_notes() {
        let steps = [
            "Install the package",
            "Start the server",
            "Open the dashboard",
            "Create an account",
            "Load the sample data",
            "Stop the server",
        ]
        .iter()
        .map(|step| format!("{step}:\n\n```sh\nrun\n```\n"))
        .collect::<Vec<_>>()
        .concat();
        let lead_ins = format!(
            "{steps}\nEach step runs in the project directory. Commands print their progress.\n"
        );
        assert_eq!(findings("VIBE008", &lead_ins), 0);
        let colons = "Input: read bytes. Output: return tokens. Limit: reject overflow. Cost: one allocation. The parser is deterministic. The scanner is parallel. Errors name the path. Tests cover failures.";
        assert_eq!(findings_at("VIBE008", "CHANGELOG.md", colons), 0);
        let comments = colons
            .split(". ")
            .map(|sentence| format!("// {sentence}.\nrun();\n"))
            .collect::<Vec<_>>()
            .concat();
        assert_eq!(findings_at("VIBE008", "src/views.rs", &comments), 0);

        let markdown = "# CSV Sum\n\n**Task:** \"Write code that sums the amount column.\"\n\nVerbatim output from a benchmark run, source `output.json`.\n\n**Using the CSV module (without pandas):**\n\nIt opens the file once.\n\n**Using pandas with error handling:**\n\nIt reports a missing file.\n\n**The pandas method is recommended** because it's:\n- More concise and readable\n- Handles data types automatically\n\nSkipped: pandas, error handling, and file closing.\n\nRows map to <file>:<line> pairs in the ledger output.\n\nThe totals match the spreadsheet.\n";
        assert_eq!(findings("VIBE008", markdown), 0);
    }

    #[test]
    fn rhythm_and_restatement_skip_lists_code_and_release_notes() {
        let rhythm = "Workers read each source file exactly one time. Rules inspect cached prose without repeated allocation overhead. Findings retain exact source offsets for stable reporting. Reporters sort every result before emitting deterministic output. Configuration changes severity without mutating detector rule logic. Git modes restrict scans to currently relevant files. Tests cover positive and negative fixture behavior carefully. Benchmarks measure complete repository scans under realistic loads.";
        let sentences: Vec<_> = rhythm.split_inclusive(". ").collect();
        let listed = sentences
            .iter()
            .map(|sentence| format!("- {}\n", sentence.trim()))
            .collect::<Vec<_>>()
            .concat();
        assert_eq!(findings("VIBE010", &listed), 0);
        let interrupted = format!(
            "{}\n\n```sh\nslopcop .\n```\n\n{}",
            sentences[..4].concat(),
            sentences[4..].concat()
        );
        assert_eq!(findings("VIBE010", &interrupted), 0);
        assert_eq!(findings_at("VIBE010", "CHANGES.rst", rhythm), 0);
        let comments = sentences
            .iter()
            .map(|sentence| format!("/// {}\nfn item() {{}}\n", sentence.trim()))
            .collect::<Vec<_>>()
            .concat();
        assert_eq!(findings_at("VIBE010", "src/lib.rs", &comments), 0);

        let triads =
            "- alpha\n- beta\n- gamma\n\n- delta\n- epsilon\n- zeta\n\n- eta\n- theta\n- iota\n";
        assert_eq!(findings_at("VIBE011", "History.md", triads), 0);

        let first =
            "The scanner reads every tracked source file in parallel during repository checks.";
        let second = "Every tracked source file is read in parallel by the scanner during repository checks.";
        assert_eq!(findings("VIBE017", &format!("- {first}\n- {second}\n")), 0);
        let rhythm = "Contrast meets the minimum for body text and headings. Hierarchy that works in light must work in dark. Brand color stays recognizable across both of the modes. Pure black and pure white flatten the depth of surfaces. Borders carry the structure where the shadows fall away. Accent color marks the single action on each screen. Motion stays short and never blocks the next input. Focus rings remain visible against every surface color.";
        assert_eq!(findings("VIBE010", rhythm), 1);
        let led_list = format!(
            "The brief decides. This skill enforces only:\n{}\n",
            format!("{rhythm} Icons share one stroke weight across the whole interface.")
                .split_inclusive(". ")
                .map(|sentence| format!("* {}", sentence.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert_eq!(findings("VIBE010", &led_list), 0, "{led_list}");
        let table = format!(
            "| Step | Behavior |\n| --- | --- |\n| read | {first} |\n| scan | {second} |\n"
        );
        assert_eq!(
            findings("VIBE017", &table),
            0,
            "table rows are parallel by design"
        );
        assert_eq!(
            findings_at("VIBE017", "CHANGELOG.md", &format!("{first} {second}")),
            0
        );
        let separate = format!("/// {first}\nfn read() {{}}\n\n/// {second}\nfn scan() {{}}\n");
        assert_eq!(findings_at("VIBE017", "src/lib.rs", &separate), 0);
        let same_comment = format!("/// {first}\n/// {second}\nfn read() {{}}\n");
        assert_eq!(findings_at("VIBE017", "src/lib.rs", &same_comment), 1);
    }

    #[test]
    fn paragraph_symmetry_requires_one_uninterrupted_document_passage() {
        let paragraph = "Each paragraph contains exactly enough ordinary words to cross the substantial length threshold while describing one measured behavior clearly today.";
        let walkthrough = std::iter::repeat_n(paragraph, 5)
            .collect::<Vec<_>>()
            .join("\n\n```sh\nslopcop .\n```\n\n");
        assert_eq!(findings("VIBE018", &walkthrough), 0);
        let comments = format!("// {paragraph}\nrun();\n").repeat(5);
        assert_eq!(findings_at("VIBE018", "src/app.ts", &comments), 0);
    }

    #[test]
    fn chatbot_residue_needs_only_one_unquoted_artifact() {
        assert_eq!(
            findings(
                "VIBE019",
                "As an AI language model, I cannot browse the internet."
            ),
            1
        );
        assert_eq!(
            findings(
                "VIBE019",
                r#"Avoid the phrase "as an AI language model" in generated documentation."#
            ),
            0
        );
        assert_eq!(
            findings(
                "VIBE019",
                "The model card records the training-data cutoff and evaluation date."
            ),
            0
        );
    }

    #[test]
    fn rhetorical_contrasts_need_a_dense_varied_cluster() {
        let cluster = "It's a cache, not a database. The goal isn't speed; it's predictability. Rather than tuning the size, measure the misses.";
        assert_eq!(findings("VIBE020", cluster), 1);
        assert_eq!(
            findings(
                "VIBE020",
                "This isn't a style rule, it's a parser limit. Instead of asking whether it is fast, ask whether it is bounded. Not just the parser, but the lexer too."
            ),
            1
        );
        assert_eq!(
            findings(
                "VIBE020",
                "It\u{2019}s a cache, not a database. There\u{2019}s a real difference between a miss and an error. Speed isn\u{2019}t the point; bounded latency is."
            ),
            1
        );
        assert_eq!(
            findings_at(
                "VIBE020",
                "src/cache.rs",
                "// It's a cache, not a database.\n// The goal isn't speed; it's predictability.\n// Rather than tuning the size, measure the misses.\n"
            ),
            1
        );
    }

    #[test]
    fn rhetorical_contrasts_ignore_sparse_repeated_quoted_and_release_note_uses() {
        assert_eq!(
            findings(
                "VIBE020",
                "It is a cache, not a database. Rather than tuning the size, measure the misses."
            ),
            0
        );
        let repeated = "It's a cache, not a database. It's a hint, not a guarantee. It's a draft, not a release.";
        assert_eq!(findings("VIBE020", repeated), 0);
        let cluster = "It's a cache, not a database. The goal isn't speed; it's predictability. Rather than tuning the size, measure the misses.";
        let filler =
            " The cache stores rendered pages and rebuilds them from the origin.".repeat(80);
        assert_eq!(findings("VIBE020", &format!("{cluster}{filler}")), 0);
        assert_eq!(
            findings(
                "VIBE020",
                r#"Avoid "It's X, not Y." Avoid "The goal isn't X; it's Y." Avoid "Rather than X, Y.""#
            ),
            0
        );
        assert_eq!(findings_at("VIBE020", "CHANGELOG.md", cluster), 0);
    }

    #[test]
    fn phrase_hits_accept_typographic_apostrophes_and_skip_nested_phrases() {
        let phrases = ["it's worth noting", "may", "it may be helpful to"];
        assert_eq!(
            phrase_hits(
                "it\u{2019}s worth noting. it may be helpful to wait.",
                &phrases
            ),
            vec![(0, "it's worth noting"), (21, "it may be helpful to")]
        );
        assert_eq!(phrase_hits("it may help.", &phrases), vec![(3, "may")]);
    }

    #[test]
    fn assistant_framing_counts_whole_sentence_interjections_only() {
        assert_eq!(
            findings(
                "VIBE002",
                "Absolutely! Here\u{2019}s a breakdown of the options."
            ),
            1
        );
        assert_eq!(
            findings(
                "VIBE002",
                "You're absolutely right. Let me know if you'd like more."
            ),
            1
        );
        assert_eq!(
            findings(
                "VIBE002",
                "Of course, the cache can be disabled. Certainly the parser is strict. I'd recommend measuring first."
            ),
            0
        );
        assert_eq!(
            findings(
                "VIBE002",
                r#"Replies such as "Absolutely!" and "Hope this helps" are stripped."#
            ),
            0
        );
    }

    #[test]
    fn importance_labels_count_as_generic_modifiers() {
        let labels = "This essential, critical, pivotal step is paramount. The noteworthy and crucial result follows.";
        assert_eq!(findings("VIBE003", labels), 1);
        let technical = "Hold the lock in a critical section. Measure the critical path before changing the scheduler, then compare throughput, latency, and memory under the same workload.";
        assert_eq!(findings("VIBE003", technical), 0);
    }

    #[test]
    fn hedging_counts_padding_phrases_once() {
        let padding = "It can be argued that the cache helps. To some extent, it is possible that reads improve. One could argue it doesn't necessarily matter. In some respects, generally speaking, it is worth considering the cost.";
        assert_eq!(findings("VIBE004", padding), 1);
        let nested = "It may be helpful to cache. It might be worth a test. While this may be true, measure it.";
        assert_eq!(findings("VIBE004", nested), 0);
    }

    #[test]
    fn artificial_balance_counts_both_sides_lines_and_paired_hands() {
        assert_eq!(
            findings(
                "VIBE005",
                "Neither approach is inherently better. The answer depends on your goals."
            ),
            1
        );
        assert_eq!(
            findings(
                "VIBE005",
                "On the one hand, it is fast. On the other hand, it is small. On the one hand, it is new. On the other hand, it is tested."
            ),
            1
        );
        assert_eq!(
            findings(
                "VIBE005",
                "On the other hand, the parser rejects tabs. The table lists the pros and cons of each backend."
            ),
            0
        );
    }

    #[test]
    fn correlatives_need_strong_constructions() {
        assert_eq!(
            findings(
                "VIBE021",
                "Not only is it fast, but it is also clear. Not only is it small, but it is also tested."
            ),
            1
        );
        assert_eq!(
            findings(
                "VIBE021",
                "It handles everything from scripts to services. Both reads and writes are cached, whether local or remote. Both the client and the server retry."
            ),
            1
        );
        assert_eq!(
            findings(
                "VIBE021",
                "Both reads and writes are cached. Whether or not the file exists, the call succeeds. Both the client and the server retry. Choose whether to block or poll."
            ),
            0
        );
        assert_eq!(
            findings(
                "VIBE021",
                r#"Avoid "not only fast but also clear" and "everything from X to Y" in summaries."#
            ),
            0
        );
    }

    #[test]
    fn signposting_counts_structure_announcements() {
        let announcements = "There are three key points. First and foremost, the first thing to understand is scope. Before we dive in, let's look at each one. Now let's turn to cost.";
        assert_eq!(findings("VIBE006", announcements), 1);
        assert_eq!(
            findings(
                "VIBE006",
                "First, parse the header. There are three key types: `Ed25519`, `P-256`, and `RSA`."
            ),
            0
        );
    }

    #[test]
    fn colon_rule_reports_staged_reveals() {
        let reveals = "The reason is simple: caching. The result: pages load in a third of the time. There is one problem: invalidation. What changed? The index moved to memory.";
        assert_eq!(findings("VIBE008", reveals), 1);
        let labels = "Problem: the build fails. Fix: pin the compiler. Why does it fail? The lockfile is stale.\n\n## What changed?\n\nThe index moved to memory.";
        assert_eq!(findings("VIBE008", labels), 0);
        assert_eq!(findings_at("VIBE008", "CHANGELOG.md", reveals), 0);
    }

    #[test]
    fn openings_skip_comment_lines_that_open_with_code() {
        let cases = "func full() {\n\t// firstStep is the first step that has run.\n\t// For example,\n\t// 1. preStep(Success) -> step1(Success) -> step2(Running): firstStep is step1.\n\t// 2. preStep(Success) -> step1(Skipped) -> step2(Success): firstStep is step2.\n\t// 3. preStep(Success) -> step1(Running) -> step2(Waiting): firstStep is step1.\n\t// 4. preStep(Success) -> step1(Skipped) -> step2(Skipped): firstStep is nil.\n\t// 5. preStep(Success) -> step1(Cancelled) -> step2(Cancelled): firstStep is nil.\n\trun()\n}\n";
        assert_eq!(findings_at("VIBE009", "task_state.go", cases), 0);
        let prose = "func parse() {\n\t// This parser reads the header first.\n\t// This parser checks every field.\n\t// This parser rejects unknown keys.\n\t// This parser stores the result.\n\t// This parser logs each failure.\n\t// This parser returns the record.\n\trun()\n}\n";
        assert_eq!(findings_at("VIBE009", "parse.go", prose), 1);
        let fields = "struct Features {\n\tbool cube = false; /**< If true, cube arrays are supported. */\n\tbool encoders = true; /**< If true, encoders are required for arguments. */\n\tbool buffers = true; /**< If true, buffers can replace slot binding. */\n\tbool spatial = false; /**< If true, spatial scaling is supported. */\n\tbool temporal = false; /**< If true, temporal scaling is supported. */\n\tbool address = false; /**< If true, shaders can read a device address. */\n};\n";
        assert_eq!(findings_at("VIBE009", "features.h", fields), 1);
        let optional = "/*\n * Returns:\n * 1. (optional) the full width of the image, or 0 if unknown.\n * 2. (optional) the full height of the image, or 0 if unknown.\n * 3. (optional) the type of the image when it is known.\n * 4. (optional) the number of layers in the image.\n * 5. The thumbnail itself as a small static image.\n * 6. The flags that describe how it was produced.\n */\nint thumbnail(void);\n";
        assert_eq!(findings_at("VIBE009", "thumbnail.h", optional), 1);
    }

    #[test]
    fn openings_skip_release_notes() {
        let entries = "Accessing the repository API with a public token did not restrict access.\nAccessing the issue API with a public token exposed private dependencies.\nAccessing the timeline API with a public token showed private references.\nAccessing the search API with a public token listed private results.\nThe admin panel now shows the token scope.\nThe login form now rejects expired tokens.\n";
        assert_eq!(findings_at("VIBE009", "notes.md", entries), 1);
        assert_eq!(findings_at("VIBE009", "release-notes/11457.md", entries), 0);
        assert_eq!(findings_at("VIBE009", "CHANGELOG.md", entries), 0);
    }

    #[test]
    fn release_notes_may_be_one_file_per_release() {
        let reveals = "The reason is simple: caching. The result: pages load in a third of the time. There is one problem: invalidation. What changed? The index moved to memory.";
        for path in [
            "CHANGELOG/2026.3.24.md",
            "CHANGELOG/records/2026.1.12-1.md",
            "docs/releases/v0.2.0.md",
            "docs/CHANGELOG/v0.14.1/en.md",
            "docs/release-notes/2.0/README.md",
            ".github/releases/Unreleased.md",
            "release-notes-published/9.0.0.md",
            "release-notes/11457.md",
            "changelog.d/1234.bugfix.md",
        ] {
            assert_eq!(findings_at("VIBE008", path, reveals), 0, "{path}");
        }
        for path in [
            "releases/proj/README.md",
            "docs/releases/process.md",
            "docs/history/2026.3.24.md",
            "v1.2/guide.md",
            "release-notes/template.md",
            "docs/2026/report.md",
        ] {
            assert_eq!(findings_at("VIBE008", path, reveals), 1, "{path}");
        }
    }

    #[test]
    fn triads_count_nested_items_with_their_parent() {
        let options = "* Host\n  * The address where the server can be reached.\n  * Example: mydomain.com\n\n* Port\n  * The port to use when connecting.\n  * Example: 636\n\n* Filter\n  * A filter that selects administrators.\n  * Example: (objectClass=admin)\n";
        assert_eq!(findings("VIBE011", options), 0);
        let outlines = "- gitea\n    - The original migrations.\n    - Numbered in order.\n- legacy\n    - The next fifty migrations.\n    - Numbered in order.\n- current\n    - The newest migrations.\n    - Named by file.\n\n- `v14a_add-comments.go`\n- `v14a_add-emojis.go`\n- `v14b_fix-index.go`\n\n- A backported change.\n- A branch switch.\n- A manual table edit.\n";
        assert_eq!(findings("VIBE011", outlines), 0);
        let flat = "- one\n- two\n- three\n\n- four\n- five\n- six\n\n- seven\n- eight\n- nine\n";
        assert_eq!(findings("VIBE011", flat), 1);
    }

    #[test]
    fn triads_report_abstract_inline_series() {
        let inline = "The tool is simple, practical, and effective. It brings speed, reliability, and flexibility. Docs stay clear, concise, and compelling.";
        assert_eq!(findings("VIBE011", inline), 1);
        let concrete = "It runs on Linux, macOS, and Windows. Parse, validate, and store each record. Reads use the client, server, and agent tokens. Output is JSON, YAML, or TOML.";
        assert_eq!(findings("VIBE011", concrete), 0);
        assert_eq!(findings_at("VIBE011", "CHANGELOG.md", inline), 0);
    }

    #[test]
    fn vague_abstractions_include_dynamics_and_perspectives() {
        let vague = "This dynamic shapes that perspective. This shift reflects this reality. That insight explains this phenomenon. This distinction drives this mindset.";
        assert_eq!(findings("VIBE012", vague), 1);
    }

    #[test]
    fn metaphors_include_stock_idioms() {
        let idioms = "Caching is a double-edged sword and a moving target. Latency is the tip of the iceberg, a game changer, and the north star that helps us connect the dots.";
        assert_eq!(findings("VIBE013", idioms), 1);
    }

    #[test]
    fn corporate_positivity_reports_promotional_characterizations() {
        let promo = "This exciting opportunity builds a strong foundation. It delivers valuable insights and leaves us well-positioned to grow.";
        assert_eq!(findings("VIBE014", promo), 1);
        assert_eq!(
            findings(
                "VIBE014",
                "Run the installer to elevate privileges. Profiling is a powerful tool for finding hot loops."
            ),
            0
        );
    }

    #[test]
    fn workplace_jargon_needs_a_varied_cluster() {
        let jargon = "Empower key stakeholders to leverage actionable insights, streamline workflows, and drive alignment across cross-functional teams.";
        assert_eq!(findings("VIBE022", jargon), 1);
        assert_eq!(
            findings(
                "VIBE022",
                "Use the covering index to streamline this query. The guide lists best practices for indexes."
            ),
            0
        );
    }

    #[test]
    fn ai_vocabulary_ignores_quoted_words() {
        assert_eq!(
            findings(
                "VIBE023",
                r#"Avoid "nuanced", "holistic", "meaningful", "thoughtful", and "delve" in summaries."#
            ),
            0
        );
    }

    #[test]
    fn ai_vocabulary_needs_a_varied_dense_cluster() {
        let vocabulary = "A nuanced, holistic review delves into meaningful trade-offs and offers a thoughtful, strategic perspective.";
        assert_eq!(findings("VIBE023", vocabulary), 1);
        let repeated = "Meaningful names help. Meaningful errors help. Meaningful defaults help. Meaningful logs help. Meaningful tests help.";
        assert_eq!(findings("VIBE023", repeated), 0);
        let technical = "The context carries the deadline. Each pattern matches one edge case, and every constraint is checked before the approach is chosen.";
        assert_eq!(findings("VIBE023", technical), 0);
    }

    #[test]
    fn disclaimers_include_one_size_fits_all_caveats() {
        let caveats = "There is no one-size-fits-all answer. Of course, it depends on the specific situation. Consider your individual circumstances, because every situation is different.";
        assert_eq!(findings("VIBE015", caveats), 1);
        let entries = "- Webhooks sent short refs in some cases.\n- Token scopes were not enforced in some cases.\n- DNS challenges should be smoother in some cases.\n- Mirrors failed to sync in some cases.\n";
        assert_eq!(findings_at("VIBE015", "notes.md", entries), 1);
        assert_eq!(findings_at("VIBE015", "RELEASE-NOTES.md", entries), 0);
    }

    #[test]
    fn conclusions_include_takeaways_and_reminders() {
        let closing = "The parser reads tokens and reports the first invalid byte with its offset. The scanner runs files in parallel and sorts results before printing. The key takeaway is speed. This serves as a reminder to measure. In the end, the tests pass.";
        assert_eq!(findings("VIBE016", closing), 1);
    }

    #[test]
    fn moral_framing_needs_two_distinct_lessons() {
        let lessons = "The migration took three weeks. This is a reminder that progress isn\u{2019}t always linear.";
        assert_eq!(findings("VIBE024", lessons), 1);
        assert_eq!(
            findings(
                "VIBE024",
                "The migration took three weeks. A chain is only as strong as the weakest link, so every hop is retried."
            ),
            0
        );
        assert_eq!(
            findings(
                "VIBE024",
                r#"Cut closers like "this is a reminder that" and "a testament to" from reports."#
            ),
            0
        );
    }

    #[test]
    fn dramatic_characterizations_need_two_distinct_forms() {
        let drama =
            "Version 2 marks a pivotal moment and a meaningful step forward for the project.";
        assert_eq!(findings("VIBE025", drama), 1);
        assert_eq!(
            findings(
                "VIBE025",
                "The curve has one inflection point at x = 0, where the second derivative changes sign."
            ),
            0
        );
    }

    #[test]
    fn restated_premise_needs_one_unquoted_paraphrase() {
        assert_eq!(
            findings(
                "VIBE026",
                "You\u{2019}re essentially asking whether the cache can be shared."
            ),
            1
        );
        assert_eq!(
            findings(
                "VIBE026",
                "If what you're describing is a bug, open an issue with the failing input."
            ),
            0
        );
        assert_eq!(
            findings(
                "VIBE026",
                r#"Drop openers such as "you're essentially asking" from answers."#
            ),
            0
        );
    }
}
