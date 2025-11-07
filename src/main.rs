use checkin_okxyz::{init_log_env, OktiXyz};
use std::env;
use tracing::error;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_log_env();
    let email = env::var("EMAIL").expect("EMAIL must be set");
    let passwd = env::var("PASSWD").expect("PASSWD must be set");
    let okti = OktiXyz::new(email, passwd);
    if (okti.checkin().await).is_err() {
        match okti.flush_cookie().await {
            Ok(_) => {
                okti.checkin().await?;
            }
            Err(e) => {
                error!("{:?}", e);
            }
        }
    }
    Ok(())
}
