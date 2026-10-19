//! BEHAVIOR-SPEC §4.4 at raw ingress (issue 0013).
//!
//! A batch arriving as JSON is decoded into TYPED entities before admission
//! sees it, so an opening created with `"kind":"sliding"` would fail as a
//! malformed payload and lose its §4.4 code. [`sanitize_opening_enums`]
//! inspects the raw operations first: each opening enum field that names no
//! variant is replaced by a valid placeholder and reported. The batch MUST
//! then be refused — the placeholders exist only so the rest of the batch
//! can be decoded and evaluated (on a scratch session), letting R4.4's
//! precedence choose the primary code across every failure (an
//! `entity-id-collision` or `unknown-host` outranks an invalid enum).

use domowik_format::OpeningCode;
use serde_json::Value;
use uuid::Uuid;

/// Replaces invalid enum values in every `createEntity` of an opening in
/// `operations` (a JSON array of operations) and returns their codes with
/// the entity ids; empty when there are none (or `operations` is not an
/// array — framing is the caller's to report).
pub fn sanitize_opening_enums(operations: &mut Value) -> Vec<(OpeningCode, Uuid)> {
    let mut out = Vec::new();
    let Some(ops) = operations.as_array_mut() else {
        return out;
    };
    for op in ops {
        if op.get("kind").and_then(Value::as_str) != Some("createEntity") {
            continue;
        }
        let Some(entity) = op.get_mut("entity").and_then(Value::as_object_mut) else {
            continue;
        };
        if entity.get("type").and_then(Value::as_str) != Some("opening") {
            continue;
        }
        let id = entity
            .get("id")
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or(Uuid::nil());
        for (key, code, valid, placeholder) in [
            (
                "kind",
                OpeningCode::InvalidOpeningKind,
                &["door", "window"][..],
                "door",
            ),
            (
                "side",
                OpeningCode::InvalidSide,
                &["Left", "Right"][..],
                "Left",
            ),
            (
                "hinge",
                OpeningCode::InvalidHinge,
                &["nearStart", "nearEnd"][..],
                "nearStart",
            ),
            (
                "opensToward",
                OpeningCode::InvalidOpensToward,
                &["Left", "Right"][..],
                "Left",
            ),
        ] {
            if let Some(v) = entity.get(key) {
                if !v.as_str().is_some_and(|s| valid.contains(&s)) {
                    out.push((code, id));
                    entity.insert(key.to_owned(), Value::String(placeholder.to_owned()));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_create_enums_are_reported_and_replaced() {
        let id = Uuid::now_v7();
        let mut ops = serde_json::json!([
            {"kind": "createEntity", "entity": {"type": "opening", "id": id.to_string(),
             "kind": "sliding", "side": "Left", "hinge": "middle", "opensToward": 3}},
            {"kind": "createEntity", "entity": {"type": "wall", "kind": "sliding"}},
            {"kind": "setFields", "fields": {"kind": "sliding"}},
        ]);
        let mut codes = sanitize_opening_enums(&mut ops);
        codes.sort();
        assert_eq!(
            codes,
            vec![
                (OpeningCode::InvalidOpeningKind, id),
                (OpeningCode::InvalidHinge, id),
                (OpeningCode::InvalidOpensToward, id),
            ]
        );
        assert_eq!(ops[0]["entity"]["kind"], "door");
        assert_eq!(ops[1]["entity"]["kind"], "sliding", "not an opening");
    }
}
