use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use regex::Regex;

use super::{BuiltinRule, Rule, ScanContext, emit};
use crate::analysis::{Span, word_count, words};
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
};

static VIBE008: RuleMetadata = RuleMetadata {
    id: "VIBE008",
    module: Module::Vibecheck,
    description: "Colon-heavy sentence pattern",
    default_severity: Severity::Info,
    default_confidence: Confidence::Medium,
    message: "A large share of sentences use the same colon-led structure.",
    suggestion: "Keep colons for genuine expansions and vary sentences that do not introduce a list or definition.",
    rationale: "Repeated label: explanation sentences can make prose look organized while flattening every idea into the same template.",
    examples: &["Input: one value. Output: one value. Result: one value. Reason: one value."],
    false_positives: "Glossaries and field-reference documents naturally use many colons; release notes, code comments, and lead-ins that end with a colon are excluded.",
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
    false_positives: "Procedures and intentionally parallel rhetoric may repeat openings for clarity or effect.",
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
    false_positives: "Controlled-language documentation and material written for early readers often targets uniform sentence length; lists, code comments, and release notes are excluded.",
};

static VIBE011: RuleMetadata = RuleMetadata {
    id: "VIBE011",
    module: Module::Vibecheck,
    description: "Repeated three-item list structure",
    default_severity: Severity::Info,
    default_confidence: Confidence::Medium,
    message: "The document is dominated by repeated three-item lists.",
    suggestion: "Group items by the actual shape of the material instead of forcing each section into a triad.",
    rationale: "One three-item list is normal; repeated exact triads across most of a document are a structural fingerprint worth reviewing.",
    examples: &["Three separate list groups, each containing exactly three items."],
    false_positives: "Reference material organized around a real three-part model can legitimately repeat triads; release notes are excluded.",
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
};

static VIBE014: RuleMetadata = RuleMetadata {
    id: "VIBE014",
    module: Module::Vibecheck,
    description: "Generic corporate positivity",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "Corporate positivity language is unusually dense.",
    suggestion: "Replace aspirational verbs with the observable outcome, owner, and constraint.",
    rationale: "Repeated empower, elevate, leverage, and drive-success claims can pad technical prose with untestable value language.",
    examples: &[
        "Empower teams to leverage best-in-class workflows and unlock transformative success.",
    ],
    false_positives: "Marketing pages are expected to use more benefit-oriented language than engineering references.",
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
    false_positives: "Safety, legal, and medical material can require multiple explicit disclaimers.",
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
    false_positives: "Definitions may intentionally restate a term once in equivalent language; list entries, release notes, and sentences in separate paragraphs or comments are not compared.",
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
};

static VIBE019: RuleMetadata = RuleMetadata {
    id: "VIBE019",
    module: Module::Vibecheck,
    description: "Chatbot response residue",
    default_severity: Severity::Warning,
    default_confidence: Confidence::High,
    message: "Repository prose contains an unmistakable chatbot response artifact.",
    suggestion: "Remove the assistant identity or runtime disclaimer and keep only repository-specific information.",
    rationale: "Chatbot identity, cutoff, and browsing disclaimers are interface residue rather than useful project documentation.",
    examples: &["As an AI language model, I do not have access to real-time information."],
    false_positives: "Double-quoted examples are excluded; stored chat transcripts may need a named suppression.",
};

static VIBE020: RuleMetadata = RuleMetadata {
    id: "VIBE020",
    module: Module::Vibecheck,
    description: "Rhetorical contrast formulas",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "The prose repeatedly frames claims as stock rhetorical contrasts.",
    suggestion: "State the claim directly and keep a contrast only where the rejected alternative is one a reader would actually consider.",
    rationale: "Templates such as \"it's X, not Y\" and \"the goal isn't X; it's Y\" manufacture a reversal around each claim and make prose predictable without adding information.",
    examples: &[
        "It's a cache, not a database. The goal isn't speed; it's predictability. Rather than tuning the size, measure the misses.",
    ],
    false_positives: "One or two contrasts never trigger this rule, and double-quoted examples are excluded; comparison guides that weigh named alternatives may still need configuration.",
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
    ]
}

fn boxed(metadata: &'static RuleMetadata, check_fn: super::CheckFn) -> Box<dyn Rule> {
    Box::new(BuiltinRule { metadata, check_fn })
}

fn is_prose(context: &ScanContext<'_>) -> bool {
    !matches!(
        context.source_type,
        SourceType::Configuration | SourceType::Unknown
    ) && context.prose_word_count() > 0
}

/// Rhythm and symmetry are properties of continuous prose. Comments and docstrings attached to
/// separate declarations are independent texts, so their lengths are not compared.
fn is_running_text(context: &ScanContext<'_>) -> bool {
    matches!(
        context.source_type,
        SourceType::Documentation | SourceType::Text
    ) && context.prose_word_count() > 0
}

/// Release notes repeat one entry template by design, so structural rhythm rules skip them.
fn is_release_notes(context: &ScanContext<'_>) -> bool {
    context
        .path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| {
            let stem = stem.to_ascii_lowercase().replace(['-', '_'], "");
            [
                "changelog",
                "changes",
                "history",
                "news",
                "releases",
                "releasenotes",
            ]
            .contains(&stem.as_str())
        })
}

fn paragraph_index(context: &ScanContext<'_>, offset: usize) -> Option<usize> {
    let paragraphs = context.paragraphs();
    let index = paragraphs.partition_point(|paragraph| paragraph.end <= offset);
    paragraphs
        .get(index)
        .filter(|paragraph| paragraph.start <= offset)
        .map(|_| index)
}

/// Reports whether `offset` falls in a paragraph that begins with a list item. Neighboring list
/// entries are parallel by design, as in changelogs and option references.
fn in_list_paragraph(context: &ScanContext<'_>, offset: usize) -> bool {
    paragraph_index(context, offset)
        .is_some_and(|index| is_list_item(context.paragraphs()[index].text(context.prose())))
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
        "if you'd like, i can",
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
            && !inside_double_quotes(context.prose(), sentence.start)
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

fn check_modifiers(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    const TERMS: &[&str] = &[
        "robust",
        "seamless",
        "powerful",
        "comprehensive",
        "effective",
        "efficient",
        "crucial",
        "vital",
        "important",
        "significant",
        "valuable",
        "essential",
        "critical",
        "pivotal",
        "paramount",
        "noteworthy",
    ];
    check_word_density(
        context,
        metadata,
        findings,
        TERMS,
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
    if let Some(offset) = first
        && count >= 2
    {
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

fn check_colons(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if !is_running_text(context) || is_release_notes(context) {
        return;
    }
    let sentences: Vec<_> = context
        .sentences()
        .iter()
        .filter(|span| word_count(span.text(context.prose())) >= 3)
        .collect();
    if sentences.len() < 8 {
        return;
    }
    let colon_sentences: Vec<_> = sentences
        .iter()
        .filter(|span| {
            let text = span.text(context.prose());
            text.trim_end_matches([':', '.', '!', '?']).contains(':') && !text.contains("://")
        })
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
    if !is_prose(context) {
        return;
    }
    let substantial: Vec<_> = context
        .sentences()
        .iter()
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
/// parallel by design.
fn prose_runs(
    context: &ScanContext<'_>,
    spans: &[Span],
    minimum_words: usize,
) -> Vec<Vec<(Span, usize)>> {
    let mut runs = vec![Vec::new()];
    let mut previous_end = 0;
    for span in spans {
        let listed = in_list_paragraph(context, span.start);
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
    let mut groups = Vec::new();
    let mut current = Vec::new();
    let mut offset = 0;
    let mut nonblank = 0;
    for line in context.prose().split_inclusive('\n') {
        let trimmed = line.trim_start();
        if !trimmed.trim().is_empty() {
            nonblank += 1;
        }
        if is_list_item(trimmed) {
            current.push(offset);
        } else if !current.is_empty() {
            groups.push(std::mem::take(&mut current));
        }
        offset += line.len();
    }
    if !current.is_empty() {
        groups.push(current);
    }
    let triads: Vec<_> = groups.iter().filter(|group| group.len() == 3).collect();
    let list_lines: usize = groups.iter().map(Vec::len).sum();
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

fn check_metaphors(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    const PHRASES: &[&str] = &[
        "landscape",
        "journey",
        "tapestry",
        "ecosystem",
        "navigate",
        "unlock",
        "realm",
        "cornerstone",
        "bridge the gap",
        "game-changer",
        "transformative",
        "paradigm",
    ];
    check_distinct_cluster(
        context,
        metadata,
        findings,
        PHRASES,
        DistinctThresholds {
            minimum: 5,
            distinct_minimum: 4,
            words_per_hit: 120,
        },
        "generic metaphor markers",
    );
}

fn check_corporate_positivity(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    const PHRASES: &[&str] = &[
        "drive success",
        "empower",
        "elevate",
        "foster",
        "leverage",
        "unlock potential",
        "best-in-class",
        "streamline",
        "enhance",
        "maximize value",
        "accelerate innovation",
        "deliver value",
    ];
    check_distinct_cluster(
        context,
        metadata,
        findings,
        PHRASES,
        DistinctThresholds {
            minimum: 6,
            distinct_minimum: 4,
            words_per_hit: 100,
        },
        "corporate positivity markers",
    );
}

fn check_disclaimers(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    const PHRASES: &[&str] = &[
        "it is important to note",
        "it's important to note",
        "it is worth noting",
        "it's worth noting",
        "keep in mind",
        "results may vary",
        "should be noted",
        "in some cases",
        "as with any",
        "depending on your needs",
        "depending on the context",
    ];
    check_phrase_cluster(
        context,
        metadata,
        findings,
        PHRASES,
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
    if !is_prose(context) || is_release_notes(context) {
        return;
    }
    for pair in context.sentences().windows(2) {
        let paragraph = paragraph_index(context, pair[0].start);
        if paragraph.is_none()
            || paragraph != paragraph_index(context, pair[1].start)
            || in_list_paragraph(context, pair[0].start)
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

fn check_rhetorical_contrasts(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if !is_prose(context) || is_release_notes(context) {
        return;
    }
    let prose = context.prose();
    let mut hits = Vec::new();
    for sentence in context.sentences() {
        let text = sentence.text(prose);
        // Comment markers and list bullets stay in the prose view; skip them to reach the
        // sentence's first word.
        let trimmed = text.trim_start_matches(|character: char| {
            character.is_whitespace()
                || matches!(character, '-' | '*' | '+' | '>' | '/' | '#' | '!')
        });
        let base = sentence.start + text.len() - trimmed.len();
        let trimmed = trimmed.trim_end();
        let hit = contrast_matchers()
            .iter()
            .enumerate()
            .find_map(|(template, matcher)| {
                matcher
                    .find(trimmed)
                    .map(|found| (base + found.start(), template))
            });
        if let Some((offset, template)) = hit
            && !inside_double_quotes(prose, offset)
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
    let count = tokens
        .iter()
        .filter(|word| terms.contains(&word.as_str()))
        .count();
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
            Some(format!(
                "observed {count} {label} across {} words",
                tokens.len()
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
            Some(format!(
                "observed {} {label} across {} words",
                hits.len(),
                context.prose_word_count()
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
            Some(format!(
                "observed {} {label} using {} distinct forms",
                hits.len(),
                distinct.len()
            )),
        );
    }
}

/// Finds whole-phrase occurrences in lowercase prose. A phrase written with `'` also matches a
/// typographic apostrophe, and a phrase nested inside a longer match, such as `may` inside
/// `it may be helpful to`, is not counted again.
fn phrase_hits<'a>(haystack: &str, phrases: &'a [&str]) -> Vec<(usize, &'a str)> {
    let mut hits = Vec::new();
    for phrase in phrases {
        let typographic = phrase.replace('\'', "\u{2019}");
        let spellings = if typographic == *phrase {
            vec![*phrase]
        } else {
            vec![*phrase, typographic.as_str()]
        };
        for spelling in spellings {
            let mut search_start = 0;
            while let Some(relative) = haystack[search_start..].find(spelling) {
                let offset = search_start + relative;
                let end = offset + spelling.len();
                let before = haystack[..offset].chars().next_back();
                let after = haystack[end..].chars().next();
                let boundary = |character: Option<char>| {
                    character.is_none_or(|value| !value.is_alphanumeric() && value != '_')
                };
                if boundary(before) && boundary(after) {
                    hits.push((offset, end, *phrase));
                }
                search_start = end;
            }
        }
    }
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

fn context_phrase_hits<'a>(
    context: &ScanContext<'_>,
    phrases: &'a [&str],
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
                "Empower teams to leverage and streamline work. Elevate results, enhance delivery, foster trust, and drive success.",
                "Use the index to streamline this query.",
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
        ];
        for (rule, bad, clean) in cases {
            assert_eq!(findings(rule, bad), 1, "{rule} should trigger");
            assert_eq!(findings(rule, clean), 0, "{rule} should stay quiet");
        }
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
}
