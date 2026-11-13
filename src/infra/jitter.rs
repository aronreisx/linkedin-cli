use crate::domain::models::InteractionRule;
use rand::Rng;

pub async fn human_jitter(rule: &InteractionRule) {
    let min = rule.min_delay_ms;
    let max = rule.max_delay_ms.max(min);

    let delay_ms = if min == max {
        min
    } else {
        rand::thread_rng().gen_range(min..=max)
    };

    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
}

pub async fn human_typing_delay() {
    let delay_ms = rand::thread_rng().gen_range(25..=85);
    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
}
