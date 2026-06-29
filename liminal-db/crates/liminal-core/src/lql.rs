use std::collections::HashMap;
use std::fmt;

use serde::Serialize;

use crate::reflex::ReflexCfg;

use crate::views::{ViewFilter, ViewStats};

#[derive(Debug, Clone, PartialEq)]
pub enum LqlAst {
    Select {
        pattern: String,
        window_ms: Option<u32>,
        filter: ViewFilter,
    },
    Subscribe {
        pattern: String,
        window_ms: Option<u32>,
        every_ms: Option<u32>,
        filter: ViewFilter,
    },
    Unsubscribe {
        id: u64,
    },
    IntrospectModel {
        top_n: Option<usize>,
    },
    IntrospectInfluence {
        top_n: Option<usize>,
    },
    IntrospectTension {
        top_n: Option<usize>,
    },
    IntrospectEpochs {
        top_n: Option<usize>,
    },
    IntrospectEpoch {
        id: u64,
    },
    Intend {
        tokens: Vec<String>,
        weights: Vec<f32>,
        horizon_ms: Option<u64>,
        budget: Option<f32>,
    },
    Variants {
        limit: Option<usize>,
        min_probability: Option<f32>,
    },
    SlideSet {
        id: String,
        variants: Vec<String>,
        goals: HashMap<String, f64>,
        ethics_guard: Option<String>,
    },
    Commit {
        variant_id: String,
    },
    Pivot {
        from: String,
        to: String,
        reason: Option<String>,
    },
    Defuse {
        pendulum_id: String,
        drain: f32,
    },
    ReflexOn,
    ReflexOff,
    ReflexStatus,
    ReflexTune {
        cfg: ReflexCfg,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LqlError {
    message: String,
}

impl LqlError {
    pub fn new(message: impl Into<String>) -> Self {
        LqlError {
            message: message.into(),
        }
    }
}

impl fmt::Display for LqlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for LqlError {}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LqlSelectResult {
    pub pattern: String,
    pub window_ms: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_strength: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_salience: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adreno: Option<bool>,
    pub stats: ViewStats,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LqlSubscribeResult {
    pub id: u64,
    pub pattern: String,
    pub window_ms: u32,
    pub every_ms: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_strength: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_salience: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adreno: Option<bool>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LqlUnsubscribeResult {
    pub id: u64,
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LqlIntendResult {
    pub tokens: Vec<String>,
    pub weights: Vec<f32>,
    pub horizon_ms: Option<u64>,
    pub budget: Option<f32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LqlVariantInfo {
    pub id: String,
    pub title: String,
    pub probability: f32,
    pub effort: f32,
    pub score: f32,
    pub evidence: f32,
    pub risk: f32,
    pub expected_gain: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LqlVariantsResult {
    pub limit: Option<usize>,
    pub min_probability: Option<f32>,
    pub variants: Vec<LqlVariantInfo>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LqlSlideResult {
    pub id: String,
    pub variant_ids: Vec<String>,
    pub desired_metrics: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LqlCommitResult {
    pub variant_id: String,
    pub seeds: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LqlPivotResult {
    pub from: String,
    pub to: String,
    pub inertia: f32,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LqlDefuseResult {
    pub pendulum_id: String,
    pub previous_drain: f32,
    pub new_drain: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LqlResponse {
    Select(LqlSelectResult),
    Subscribe(LqlSubscribeResult),
    Unsubscribe(LqlUnsubscribeResult),
    Intend(LqlIntendResult),
    Variants(LqlVariantsResult),
    Slide(LqlSlideResult),
    Commit(LqlCommitResult),
    Pivot(LqlPivotResult),
    Defuse(LqlDefuseResult),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LqlExecResult {
    pub response: Option<LqlResponse>,
    pub events: Vec<String>,
}

pub type LqlResult = Result<LqlExecResult, LqlError>;

pub fn parse_lql(input: &str) -> Result<LqlAst, LqlError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(LqlError::new("empty LQL query"));
    }
    let mut parts = trimmed.split_whitespace();
    let command = parts
        .next()
        .ok_or_else(|| LqlError::new("missing LQL command"))?
        .to_uppercase();
    match command.as_str() {
        "SELECT" => parse_select(&mut parts),
        "SUBSCRIBE" => parse_subscribe(&mut parts),
        "UNSUBSCRIBE" => parse_unsubscribe(&mut parts),
        "INTROSPECT" => parse_introspect(&mut parts),
        "INTEND" => {
            let tokens: Vec<&str> = parts.collect();
            parse_intend(&tokens)
        }
        "VARIANTS" => {
            let tokens: Vec<&str> = parts.collect();
            parse_variants(&tokens)
        }
        "SLIDE" => {
            let tokens: Vec<&str> = parts.collect();
            parse_slide(&tokens)
        }
        "COMMIT" => {
            let tokens: Vec<&str> = parts.collect();
            parse_commit(&tokens)
        }
        "PIVOT" => {
            let tokens: Vec<&str> = parts.collect();
            parse_pivot(&tokens)
        }
        "DEFUSE" => {
            let tokens: Vec<&str> = parts.collect();
            parse_defuse(&tokens)
        }
        "REFLEX" => parse_reflex(&mut parts),
        other => Err(LqlError::new(format!("unknown LQL command: {}", other))),
    }
}

fn parse_reflex<'a, I>(parts: &mut I) -> Result<LqlAst, LqlError>
where
    I: Iterator<Item = &'a str>,
{
    let action = parts
        .next()
        .ok_or_else(|| LqlError::new("REFLEX requires a subcommand"))?
        .to_uppercase();
    match action.as_str() {
        "ON" => Ok(LqlAst::ReflexOn),
        "OFF" => Ok(LqlAst::ReflexOff),
        "STATUS" => Ok(LqlAst::ReflexStatus),
        "TUNE" => {
            let raw: Vec<&str> = parts.collect();
            if raw.is_empty() {
                return Err(LqlError::new("REFLEX TUNE requires a config"));
            }
            let joined = raw.join(" ");
            let cfg: ReflexCfg = serde_json::from_str(&joined).map_err(|err| {
                LqlError::new(format!("invalid REFLEX config: {err}"))
            })?;
            Ok(LqlAst::ReflexTune { cfg })
        }
        other => Err(LqlError::new(format!("unknown REFLEX subcommand: {}", other))),
    }
}

fn parse_introspect<'a, I>(parts: &mut I) -> Result<LqlAst, LqlError>
where
    I: Iterator<Item = &'a str>,
{
    let target = parts
        .next()
        .ok_or_else(|| LqlError::new("INTROSPECT requires a target"))?;
    let rest: Vec<&str> = parts.collect();
    match target.to_uppercase().as_str() {
        "MODEL" => {
            let top_n = parse_top_clause(&rest)?;
            Ok(LqlAst::IntrospectModel { top_n })
        }
        "INFLUENCE" => {
            let top_n = parse_top_clause(&rest)?;
            Ok(LqlAst::IntrospectInfluence { top_n })
        }
        "TENSION" => {
            let top_n = parse_top_clause(&rest)?;
            Ok(LqlAst::IntrospectTension { top_n })
        }
        "EPOCHS" => {
            let top_n = parse_top_clause(&rest)?;
            Ok(LqlAst::IntrospectEpochs { top_n })
        }
        "EPOCH" => {
            if rest.is_empty() {
                return Err(LqlError::new("INTROSPECT EPOCH requires an id"));
            }
            if rest.len() > 1 {
                return Err(LqlError::new("INTROSPECT EPOCH takes a single id"));
            }
            let id: u64 = rest[0]
                .parse()
                .map_err(|_| LqlError::new("INTROSPECT EPOCH id must be numeric"))?;
            Ok(LqlAst::IntrospectEpoch { id })
        }
        other => Err(LqlError::new(format!(
            "unexpected INTROSPECT target: {}",
            other
        ))),
    }
}

fn parse_top_clause(tokens: &[&str]) -> Result<Option<usize>, LqlError> {
    if tokens.is_empty() {
        return Ok(None);
    }
    if !tokens[0].eq_ignore_ascii_case("TOP") {
        return Err(LqlError::new(format!(
            "unexpected token in INTROSPECT: {}",
            tokens[0]
        )));
    }
    if tokens.len() < 2 {
        return Err(LqlError::new("TOP requires a numeric value"));
    }
    if tokens.len() > 2 {
        return Err(LqlError::new("unexpected tokens after TOP clause"));
    }
    let value: usize = tokens[1]
        .parse()
        .map_err(|_| LqlError::new("TOP requires a positive integer"))?;
    if value == 0 {
        return Err(LqlError::new("TOP must be greater than zero"));
    }
    Ok(Some(value))
}

fn parse_where_conditions<'a>(
    tokens: &[&'a str],
    filter: &mut ViewFilter,
) -> Result<usize, LqlError> {
    if tokens.is_empty() {
        return Err(LqlError::new("WHERE requires a condition"));
    }
    let mut consumed = 0usize;
    let mut expect_condition = true;
    while consumed < tokens.len() {
        let token = tokens[consumed];
        let upper = token.to_uppercase();
        if upper == "WINDOW" || upper == "EVERY" {
            break;
        }
        if !expect_condition {
            if upper == "AND" {
                consumed += 1;
                expect_condition = true;
                continue;
            }
            return Err(LqlError::new("expected AND between WHERE clauses"));
        }
        parse_condition_token(token, filter)?;
        consumed += 1;
        expect_condition = false;
    }
    if consumed == 0 {
        return Err(LqlError::new("WHERE requires a condition"));
    }
    if expect_condition {
        return Err(LqlError::new("WHERE requires a condition"));
    }
    Ok(consumed)
}

fn parse_condition_token(token: &str, filter: &mut ViewFilter) -> Result<(), LqlError> {
    if let Some(value) = token.strip_prefix("strength>=") {
        let strength: f32 = value
            .parse()
            .map_err(|_| LqlError::new("invalid strength threshold"))?;
        if !(0.0..=1.0).contains(&strength) {
            return Err(LqlError::new("strength must be within 0.0..=1.0"));
        }
        filter.min_strength = Some(strength);
        return Ok(());
    }
    if let Some(value) = token.strip_prefix("salience>=") {
        let salience: f32 = value
            .parse()
            .map_err(|_| LqlError::new("invalid salience threshold"))?;
        if !(0.0..=1.0).contains(&salience) {
            return Err(LqlError::new("salience must be within 0.0..=1.0"));
        }
        filter.min_salience = Some(salience);
        return Ok(());
    }
    if let Some(value) = token.strip_prefix("adreno=") {
        match value.to_lowercase().as_str() {
            "true" => {
                filter.adreno = Some(true);
                return Ok(());
            }
            "false" => {
                filter.adreno = Some(false);
                return Ok(());
            }
            _ => {
                return Err(LqlError::new("adreno must be true or false"));
            }
        }
    }
    Err(LqlError::new("unsupported WHERE clause"))
}

fn parse_select<'a, I>(parts: &mut I) -> Result<LqlAst, LqlError>
where
    I: Iterator<Item = &'a str>,
{
    let pattern = parts
        .next()
        .ok_or_else(|| LqlError::new("SELECT requires a pattern"))?;
    let mut window_ms = None;
    let mut filter = ViewFilter::default();
    let rest: Vec<&str> = parts.collect();
    let mut i = 0;
    while i < rest.len() {
        match rest[i].to_uppercase().as_str() {
            "WHERE" => {
                i += 1;
                let consumed = parse_where_conditions(&rest[i..], &mut filter)?;
                i += consumed;
            }
            "WINDOW" => {
                i += 1;
                if i >= rest.len() {
                    return Err(LqlError::new("WINDOW requires milliseconds value"));
                }
                let value: u32 = rest[i]
                    .parse()
                    .map_err(|_| LqlError::new("invalid WINDOW value"))?;
                if value == 0 {
                    return Err(LqlError::new("WINDOW must be greater than zero"));
                }
                window_ms = Some(value);
                i += 1;
                continue;
            }
            other => {
                return Err(LqlError::new(format!(
                    "unexpected token in SELECT: {}",
                    other
                )));
            }
        }
    }
    Ok(LqlAst::Select {
        pattern: pattern.to_string(),
        window_ms,
        filter,
    })
}

fn parse_subscribe<'a, I>(parts: &mut I) -> Result<LqlAst, LqlError>
where
    I: Iterator<Item = &'a str>,
{
    let pattern = parts
        .next()
        .ok_or_else(|| LqlError::new("SUBSCRIBE requires a pattern"))?;
    let mut window_ms = None;
    let mut every_ms = None;
    let mut filter = ViewFilter::default();
    let rest: Vec<&str> = parts.collect();
    let mut i = 0;
    while i < rest.len() {
        match rest[i].to_uppercase().as_str() {
            "WHERE" => {
                i += 1;
                let consumed = parse_where_conditions(&rest[i..], &mut filter)?;
                i += consumed;
            }
            "WINDOW" => {
                i += 1;
                if i >= rest.len() {
                    return Err(LqlError::new("WINDOW requires milliseconds value"));
                }
                let value: u32 = rest[i]
                    .parse()
                    .map_err(|_| LqlError::new("invalid WINDOW value"))?;
                if value == 0 {
                    return Err(LqlError::new("WINDOW must be greater than zero"));
                }
                window_ms = Some(value);
                i += 1;
                continue;
            }
            "EVERY" => {
                i += 1;
                if i >= rest.len() {
                    return Err(LqlError::new("EVERY requires milliseconds value"));
                }
                let value: u32 = rest[i]
                    .parse()
                    .map_err(|_| LqlError::new("invalid EVERY value"))?;
                if value == 0 {
                    return Err(LqlError::new("EVERY must be greater than zero"));
                }
                every_ms = Some(value);
                i += 1;
                continue;
            }
            other => {
                return Err(LqlError::new(format!(
                    "unexpected token in SUBSCRIBE: {}",
                    other
                )));
            }
        }
    }
    Ok(LqlAst::Subscribe {
        pattern: pattern.to_string(),
        window_ms,
        every_ms,
        filter,
    })
}

fn parse_unsubscribe<'a, I>(parts: &mut I) -> Result<LqlAst, LqlError>
where
    I: Iterator<Item = &'a str>,
{
    let id_raw = parts
        .next()
        .ok_or_else(|| LqlError::new("UNSUBSCRIBE requires an id"))?;
    if parts.next().is_some() {
        return Err(LqlError::new("UNSUBSCRIBE takes exactly one argument"));
    }
    let id: u64 = id_raw
        .parse()
        .map_err(|_| LqlError::new("invalid view id"))?;
    Ok(LqlAst::Unsubscribe { id })
}

fn parse_intend(tokens: &[&str]) -> Result<LqlAst, LqlError> {
    if tokens.is_empty() {
        return Err(LqlError::new("INTEND requires arguments"));
    }
    let mut focus_tokens: Option<Vec<String>> = None;
    let mut weights: Option<Vec<f32>> = None;
    let mut horizon_ms: Option<u64> = None;
    let mut budget: Option<f32> = None;
    let mut idx = 0;
    while idx < tokens.len() {
        let key = tokens[idx].to_uppercase();
        idx += 1;
        match key.as_str() {
            "TOKENS" => {
                let raw = tokens
                    .get(idx)
                    .ok_or_else(|| LqlError::new("INTEND TOKENS requires a list"))?;
                idx += 1;
                focus_tokens = Some(parse_string_array(raw)?);
            }
            "WEIGHTS" => {
                let raw = tokens
                    .get(idx)
                    .ok_or_else(|| LqlError::new("INTEND WEIGHTS requires a list"))?;
                idx += 1;
                weights = Some(parse_float_array(raw)?);
            }
            "HORIZON" => {
                let raw = tokens
                    .get(idx)
                    .ok_or_else(|| LqlError::new("INTEND HORIZON requires milliseconds"))?;
                idx += 1;
                horizon_ms = Some(
                    raw.parse::<u64>()
                        .map_err(|_| LqlError::new("INTEND HORIZON expects a positive integer"))?,
                );
            }
            "BUDGET" => {
                let raw = tokens
                    .get(idx)
                    .ok_or_else(|| LqlError::new("INTEND BUDGET requires a value"))?;
                idx += 1;
                budget = Some(
                    raw.parse::<f32>()
                        .map_err(|_| LqlError::new("INTEND BUDGET expects a numeric value"))?,
                );
            }
            other => {
                return Err(LqlError::new(format!("unexpected INTEND token: {}", other)));
            }
        }
    }
    let focus_tokens = focus_tokens.ok_or_else(|| LqlError::new("INTEND requires TOKENS"))?;
    let weights = weights.unwrap_or_else(|| {
        if focus_tokens.is_empty() {
            Vec::new()
        } else {
            vec![1.0 / focus_tokens.len() as f32; focus_tokens.len()]
        }
    });
    if weights.len() != focus_tokens.len() {
        return Err(LqlError::new(
            "INTEND TOKENS and WEIGHTS must have the same length",
        ));
    }
    Ok(LqlAst::Intend {
        tokens: focus_tokens,
        weights,
        horizon_ms,
        budget,
    })
}

fn parse_variants(tokens: &[&str]) -> Result<LqlAst, LqlError> {
    if tokens.is_empty() {
        return Ok(LqlAst::Variants {
            limit: None,
            min_probability: None,
        });
    }
    let mut limit: Option<usize> = None;
    let mut min_probability: Option<f32> = None;
    let first = tokens[0].to_uppercase();
    if first == "TOP" {
        if tokens.len() < 2 {
            return Err(LqlError::new("VARIANTS TOP requires a number"));
        }
        let parsed: usize = tokens[1]
            .parse()
            .map_err(|_| LqlError::new("VARIANTS TOP expects a positive integer"))?;
        if parsed == 0 {
            return Err(LqlError::new("VARIANTS TOP must be greater than zero"));
        }
        limit = Some(parsed);
    } else if first == "SELECT" {
        let mut idx = 1;
        while idx < tokens.len() {
            let key = tokens[idx].to_uppercase();
            idx += 1;
            match key.as_str() {
                "WHERE" => {
                    let Some(condition) = tokens.get(idx) else {
                        return Err(LqlError::new(
                            "VARIANTS WHERE requires a condition like p>=0.5",
                        ));
                    };
                    idx += 1;
                    if let Some(value) = condition.strip_prefix("p>=") {
                        let parsed = value.parse::<f32>().map_err(|_| {
                            LqlError::new("VARIANTS WHERE p>= expects a numeric value")
                        })?;
                        min_probability = Some(parsed.clamp(0.0, 1.0));
                    } else {
                        return Err(LqlError::new(format!(
                            "unsupported VARIANTS WHERE condition: {}",
                            condition
                        )));
                    }
                    continue;
                }
                "LIMIT" => {
                    let Some(value) = tokens.get(idx) else {
                        return Err(LqlError::new("VARIANTS LIMIT requires a number"));
                    };
                    idx += 1;
                    let parsed: usize = value
                        .parse()
                        .map_err(|_| LqlError::new("VARIANTS LIMIT expects an integer"))?;
                    if parsed == 0 {
                        return Err(LqlError::new("VARIANTS LIMIT must be greater than zero"));
                    }
                    limit = Some(parsed);
                    continue;
                }
                _ => {}
            }
        }
    } else {
        return Err(LqlError::new(format!(
            "unexpected VARIANTS directive: {}",
            tokens[0]
        )));
    }
    Ok(LqlAst::Variants {
        limit,
        min_probability,
    })
}

fn parse_slide(tokens: &[&str]) -> Result<LqlAst, LqlError> {
    if tokens.len() < 4 {
        return Err(LqlError::new("SLIDE SET requires id and variant list"));
    }
    if !tokens[0].eq_ignore_ascii_case("SET") {
        return Err(LqlError::new("expected SLIDE SET"));
    }
    let id = trim_quotes(tokens[1]);
    let mut idx = 2;
    let mut variant_ids: Vec<String> = Vec::new();
    let mut desired_metrics: HashMap<String, f64> = HashMap::new();
    let mut ethics_guard: Option<String> = None;
    while idx < tokens.len() {
        let key = tokens[idx].to_uppercase();
        idx += 1;
        match key.as_str() {
            "VARIANTS" => {
                let raw = tokens
                    .get(idx)
                    .ok_or_else(|| LqlError::new("SLIDE VARIANTS requires a list"))?;
                idx += 1;
                variant_ids = parse_variant_list(raw);
            }
            "GOALS" => {
                let raw = tokens
                    .get(idx)
                    .ok_or_else(|| LqlError::new("SLIDE GOALS requires a map"))?;
                idx += 1;
                desired_metrics = serde_json::from_str(raw).map_err(|_| {
                    LqlError::new("SLIDE GOALS expects a JSON object with numeric values")
                })?;
            }
            "ETHICS" => {
                let raw = tokens
                    .get(idx)
                    .ok_or_else(|| LqlError::new("SLIDE ETHICS requires a guard"))?;
                idx += 1;
                ethics_guard = Some(trim_quotes(raw));
            }
            other => {
                return Err(LqlError::new(format!("unexpected SLIDE token: {}", other)));
            }
        }
    }
    if variant_ids.is_empty() {
        return Err(LqlError::new("SLIDE requires at least one variant"));
    }
    Ok(LqlAst::SlideSet {
        id,
        variants: variant_ids,
        goals: desired_metrics,
        ethics_guard,
    })
}

fn parse_commit(tokens: &[&str]) -> Result<LqlAst, LqlError> {
    if tokens.is_empty() {
        return Err(LqlError::new("COMMIT requires a variant id"));
    }
    let variant_id = trim_quotes(tokens[0]);
    Ok(LqlAst::Commit { variant_id })
}

fn parse_pivot(tokens: &[&str]) -> Result<LqlAst, LqlError> {
    if tokens.is_empty() {
        return Err(LqlError::new("PIVOT requires arguments"));
    }
    let from_to = trim_quotes(tokens[0]);
    let mut reason: Option<String> = None;
    if let Some(pos) = tokens.iter().position(|t| t.eq_ignore_ascii_case("REASON")) {
        let message = tokens[pos + 1..]
            .iter()
            .map(|t| trim_quotes(t))
            .collect::<Vec<_>>()
            .join(" ");
        if !message.is_empty() {
            reason = Some(message);
        }
    }
    let parts: Vec<&str> = from_to.split("->").collect();
    if parts.len() != 2 {
        return Err(LqlError::new("PIVOT expects format from->to"));
    }
    Ok(LqlAst::Pivot {
        from: parts[0].to_string(),
        to: parts[1].to_string(),
        reason,
    })
}

fn parse_defuse(tokens: &[&str]) -> Result<LqlAst, LqlError> {
    if tokens.is_empty() {
        return Err(LqlError::new("DEFUSE requires a pendulum id"));
    }
    let pendulum_id = trim_quotes(tokens[0]);
    let mut drain = 0.3f32;
    if tokens.len() >= 3 {
        let key = tokens[1].to_uppercase();
        if key == "RATE" || key == "DRAIN" {
            drain = tokens[2]
                .parse::<f32>()
                .map_err(|_| LqlError::new("DEFUSE RATE expects a number"))?;
        }
    }
    Ok(LqlAst::Defuse { pendulum_id, drain })
}

fn parse_string_array(raw: &str) -> Result<Vec<String>, LqlError> {
    serde_json::from_str::<Vec<String>>(raw).map_err(|_| LqlError::new("expected string array"))
}

fn parse_float_array(raw: &str) -> Result<Vec<f32>, LqlError> {
    let values: Vec<f32> =
        serde_json::from_str(raw).map_err(|_| LqlError::new("expected numeric array"))?;
    Ok(values)
}

fn parse_variant_list(raw: &str) -> Vec<String> {
    raw.trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .filter_map(|part| {
            let trimmed = trim_quotes(part.trim());
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .collect()
}

fn trim_quotes(input: &str) -> String {
    input.trim_matches(|c| c == '\"' || c == '\'').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_select_basic() {
        let ast = parse_lql("SELECT cpu/load WINDOW 1500").unwrap();
        assert_eq!(
            ast,
            LqlAst::Select {
                pattern: "cpu/load".into(),
                window_ms: Some(1500),
                filter: ViewFilter::default(),
            }
        );
    }

    #[test]
    fn parse_select_with_where() {
        let ast = parse_lql("SELECT cpu/load WHERE strength>=0.7 WINDOW 1200").unwrap();
        let mut filter = ViewFilter::default();
        filter.min_strength = Some(0.7);
        assert_eq!(
            ast,
            LqlAst::Select {
                pattern: "cpu/load".into(),
                window_ms: Some(1200),
                filter,
            }
        );
    }

    #[test]
    fn parse_subscribe_every() {
        let ast = parse_lql("SUBSCRIBE mem/free WINDOW 2000 EVERY 1000").unwrap();
        assert_eq!(
            ast,
            LqlAst::Subscribe {
                pattern: "mem/free".into(),
                window_ms: Some(2000),
                every_ms: Some(1000),
                filter: ViewFilter::default(),
            }
        );
    }

    #[test]
    fn parse_unsubscribe() {
        let ast = parse_lql("UNSUBSCRIBE 42").unwrap();
        assert_eq!(ast, LqlAst::Unsubscribe { id: 42 });
    }

    #[test]
    fn reject_invalid_strength() {
        let err = parse_lql("SELECT cpu WHERE strength>=1.5").unwrap_err();
        assert!(err.to_string().contains("strength"));
    }

    #[test]
    fn reject_unknown_command() {
        let err = parse_lql("FOO bar").unwrap_err();
        assert!(err.to_string().contains("unknown"));
    }

    #[test]
    fn parse_select_with_extended_filters() {
        let ast = parse_lql("SELECT cpu/load WHERE salience>=0.7 AND adreno=true WINDOW 5000")
            .expect("parse select");
        match ast {
            LqlAst::Select {
                pattern,
                window_ms,
                filter,
            } => {
                assert_eq!(pattern, "cpu/load");
                assert_eq!(window_ms, Some(5_000));
                assert_eq!(filter.min_salience, Some(0.7));
                assert_eq!(filter.adreno, Some(true));
            }
            _ => panic!("unexpected AST variant"),
        }
    }

    #[test]
    fn parse_subscribe_with_adreno_filter() {
        let ast = parse_lql("SUBSCRIBE * WHERE adreno=false WINDOW 1000 EVERY 5000")
            .expect("parse subscribe");
        match ast {
            LqlAst::Subscribe {
                pattern,
                window_ms,
                every_ms,
                filter,
            } => {
                assert_eq!(pattern, "*");
                assert_eq!(window_ms, Some(1_000));
                assert_eq!(every_ms, Some(5_000));
                assert_eq!(filter.adreno, Some(false));
            }
            _ => panic!("unexpected AST variant"),
        }
    }

    #[test]
    fn parse_introspect_model_without_top() {
        let ast = parse_lql("INTROSPECT model").expect("parse introspect");
        assert_eq!(ast, LqlAst::IntrospectModel { top_n: None });
    }

    #[test]
    fn parse_introspect_influence_with_top() {
        let ast = parse_lql("INTROSPECT influence TOP 7").expect("parse introspect");
        assert_eq!(ast, LqlAst::IntrospectInfluence { top_n: Some(7) });
    }

    #[test]
    fn parse_introspect_rejects_invalid_top() {
        let err = parse_lql("INTROSPECT model TOP 0").unwrap_err();
        assert!(err.to_string().contains("TOP"));
    }

    #[test]
    fn parse_introspect_rejects_unknown_target() {
        let err = parse_lql("INTROSPECT foo").unwrap_err();
        assert!(err.to_string().contains("unexpected"));
    }
}
