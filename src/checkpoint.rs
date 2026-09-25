use crate::config::{CheckpointHeaderCondition, HeaderValueMatch};
use crate::landing::{read_landing_asset_by_rel_path_async, send_page_response};
use crate::nginx_async::perform_async;
use crate::prelude::*;
use crate::Module;
use ngx::core::Status;
use ngx::http::{HTTPStatus, HttpModule, Method};

pub fn should_serve_checkpoint(enabled: bool, state: ServiceHealthState, method: Method) -> bool {
    enabled && state != ServiceHealthState::Starting && method != Method::POST
}

pub fn checkpoint_bypass_matches(
    rules: &[Vec<CheckpointHeaderCondition>],
    request: &Request,
) -> bool {
    if rules.is_empty() {
        return false;
    }
    let headers: Vec<_> = request
        .headers_in_iterator()
        .map(|(name, value)| (name.as_ref(), value.as_ref()))
        .collect();
    checkpoint_bypass_matches_headers(rules, &headers)
}

fn checkpoint_bypass_matches_headers(
    rules: &[Vec<CheckpointHeaderCondition>],
    headers: &[(&[u8], &[u8])],
) -> bool {
    rules.iter().any(|rule| {
        rule.iter().all(|condition| {
            headers.iter().any(|(name, value)| {
                name.eq_ignore_ascii_case(condition.name.as_bytes())
                    && condition.value_match.as_ref().is_none_or(|matcher| match matcher {
                        HeaderValueMatch::StartsWith(prefix) => {
                            value.starts_with(prefix.as_bytes())
                        }
                        HeaderValueMatch::Contains(part) => value
                            .windows(part.len())
                            .any(|window| window == part.as_bytes()),
                        HeaderValueMatch::EndsWith(suffix) => {
                            value.ends_with(suffix.as_bytes())
                        }
                    })
            })
        })
    })
}

pub fn serve_checkpoint_page(request: &mut Request, landing_dir: &str) -> Status {
    let dir = landing_dir.to_owned();
    let result = perform_async(request, Module::module(), || async move {
        read_landing_asset_by_rel_path_async(&dir, "checkpoint.html").await
    });

    let Some(result) = result else {
        return Status::NGX_AGAIN;
    };

    let (body, content_type) =
        result.unwrap_or_else(|| (b"Site is unavailable.\n".to_vec(), "text/plain"));
    send_page_response(
        request,
        &body,
        content_type,
        HTTPStatus::SERVICE_UNAVAILABLE,
    )
}

#[cfg(test)]
mod tests {
    use super::{checkpoint_bypass_matches_headers, CheckpointHeaderCondition, HeaderValueMatch};

    fn presence(name: &str) -> CheckpointHeaderCondition {
        CheckpointHeaderCondition {
            name: name.to_owned(),
            value_match: None,
        }
    }

    fn matching(name: &str, matcher: HeaderValueMatch) -> CheckpointHeaderCondition {
        CheckpointHeaderCondition {
            name: name.to_owned(),
            value_match: Some(matcher),
        }
    }

    #[test]
    fn checkpoint_bypass_requires_all_conditions_in_a_matching_rule() {
        let rules = vec![
            vec![presence("X-First"), presence("X-Second")],
            vec![presence("X-Alternative")],
        ];

        assert!(!checkpoint_bypass_matches_headers(
            &rules,
            &[(b"X-First", b"yes")]
        ));
        assert!(checkpoint_bypass_matches_headers(
            &rules,
            &[(b"X-First", b"yes"), (b"X-Second", b"yes")]
        ));
        assert!(checkpoint_bypass_matches_headers(
            &rules,
            &[(b"X-Alternative", b"yes")]
        ));
    }

    #[test]
    fn checkpoint_bypass_matches_header_values() {
        let rules = vec![vec![
            matching(
                "X-Token",
                HeaderValueMatch::StartsWith("trusted-".to_owned()),
            ),
            matching("X-Trace", HeaderValueMatch::Contains("internal".to_owned())),
            matching("X-Client", HeaderValueMatch::EndsWith("-agent".to_owned())),
        ]];

        assert!(checkpoint_bypass_matches_headers(
            &rules,
            &[
                (b"x-token", b"trusted-123"),
                (b"X-Trace", b"my-internal-request"),
                (b"X-Client", b"test-agent"),
            ]
        ));
        assert!(!checkpoint_bypass_matches_headers(
            &rules,
            &[
                (b"X-Token", b"untrusted-123"),
                (b"X-Trace", b"my-internal-request"),
                (b"X-Client", b"test-agent"),
            ]
        ));
    }
}
