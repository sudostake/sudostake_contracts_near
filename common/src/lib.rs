#[macro_export]
macro_rules! log_event {
    ($event_name:expr, $data:expr) => {
        near_sdk::env::log_str(
            &format!(
                "EVENT_JSON:{}",
                near_sdk::serde_json::json!({
                    "event": $event_name,
                    "data": $data
                })
            ),
        );
    };
}
