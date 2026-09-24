#![forbid(unsafe_code)]

//! The context route technology — a technology of `xmip-core-route`.
//!
//! A Subscription's filter names properties, and each property is read from
//! one source. This source reads a context value: `context:<key>` is what the
//! Message's Context holds under `<key>`, rendered as the text a filter
//! compares — text as it is, a boolean as `true` or `false`, a number as it
//! prints. No prefix is context too, which is what routing has always read; the
//! explicit spelling exists for the two readings the implicit one cannot give.
//! Both spellings read through `route::routable`, so they agree: a key the
//! Context does not hold and a `Null` are absent — nothing promoted, so a
//! filter over it declines with its reason, `exists` fails, and empty text
//! does not match. A `Binary` value is refused with a reason, because bytes
//! are not text. ADR-0046, amended 2026-09-24.
//!
//! A route technology does not decide anything: it reads.

use message::Message;
use route::{Source, SourceError};

/// Reads `context:<key>` as the typed context value rendered as text.
pub struct ContextSource;

impl Source for ContextSource {
    fn technology(&self) -> &'static str {
        route::CONTEXT
    }

    fn read(&self, message: &Message, name: &str) -> Result<Option<String>, SourceError> {
        if name.is_empty() {
            return Err(SourceError::new(
                route::CONTEXT,
                name,
                "a context key is needed after the prefix",
            ));
        }

        route::routable(name, message.context().get(name))
            .map_err(|reason| SourceError::new(route::CONTEXT, name, reason))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use context::{ContextValue, MessageContext};
    use message::MessageTreatment;
    use route::{Predicate, Value};
    use xcore::MessageId;

    fn message() -> Message {
        let context = MessageContext::new()
            .with_value("MessageType", ContextValue::Text("Order".into()))
            .with_value("Amount", ContextValue::Integer(1500))
            .with_value("Urgent", ContextValue::Bool(true))
            .with_value("Weight", ContextValue::Decimal(2.5))
            .with_value("Note", ContextValue::Null)
            .with_value("Blob", ContextValue::Binary(vec![0, 1, 2]));
        Message::received(
            MessageId::new(1),
            Vec::new(),
            context,
            MessageTreatment::default(),
        )
    }

    fn read(name: &str) -> Result<Option<String>, SourceError> {
        ContextSource.read(&message(), name)
    }

    #[test]
    fn text_a_boolean_and_numbers_read_as_the_text_a_filter_compares() {
        assert_eq!(read("MessageType").expect("text"), Some("Order".into()));
        assert_eq!(read("Amount").expect("integer"), Some("1500".into()));
        assert_eq!(read("Urgent").expect("boolean"), Some("true".into()));
        assert_eq!(read("Weight").expect("decimal"), Some("2.5".into()));
    }

    #[test]
    fn an_absent_key_and_a_null_value_read_as_nothing_promoted() {
        assert_eq!(read("Region").expect("readable"), None);
        assert_eq!(read("Note").expect("readable"), None);
    }

    #[test]
    fn bytes_and_an_empty_key_are_refused_with_a_reason() {
        let refused = read("Blob").expect_err("bytes");
        assert_eq!(refused.technology, "context");
        assert_eq!(refused.property, "Blob");
        assert!(refused.reason.contains("3 bytes"));

        let empty = read("").expect_err("no key");
        assert!(empty.reason.contains("context key"));
    }

    #[test]
    fn the_technology_is_context_and_promote_reads_the_prefixed_property() {
        assert_eq!(ContextSource.technology(), "context");

        let sources: [&dyn Source; 1] = [&ContextSource];
        let promoted = route::promote(
            &message(),
            &sources,
            &["context:Amount", "context:Note", "MessageType"],
        )
        .expect("readable");

        assert_eq!(promoted.get("context:Amount"), Some("1500"));
        assert_eq!(promoted.get("context:Note"), None);
        assert_eq!(promoted.get("MessageType"), Some("Order"));
        assert!(
            Predicate::greater_than("context:Amount", Value::Integer(1000))
                .test(&promoted)
                .passed()
        );
    }

    #[test]
    fn both_spellings_agree_on_present_missing_null_and_bytes() {
        let sources: [&dyn Source; 1] = [&ContextSource];
        let spellings: [(&[&dyn Source], &str); 3] =
            [(&sources, ""), (&sources, "context:"), (&[], "context:")];

        for (loaded, prefix) in spellings {
            let named = |key: &str| format!("{prefix}{key}");
            let (present, missing, null) = (named("Amount"), named("Region"), named("Note"));
            let promoted = route::promote(
                &message(),
                loaded,
                &[present.as_str(), missing.as_str(), null.as_str()],
            )
            .expect("readable");

            assert_eq!(promoted.get(&present), Some("1500"), "{present}");
            assert_eq!(promoted.get(&missing), None, "{missing}");
            assert_eq!(promoted.get(&null), None, "{null}");
            assert!(Predicate::exists(present.clone()).test(&promoted).passed());
            assert!(!Predicate::exists(missing.clone()).test(&promoted).passed());
            assert!(!Predicate::exists(null.clone()).test(&promoted).passed());
            assert!(
                !Predicate::equals(null.clone(), Value::Text(String::new()))
                    .test(&promoted)
                    .passed(),
                "{null} is absent, not empty text"
            );

            let refused =
                route::promote(&message(), loaded, &[named("Blob").as_str()]).expect_err("bytes");
            assert_eq!(refused.technology, "context");
            assert!(refused.reason.contains("Blob holds 3 bytes"));
        }
    }
}
