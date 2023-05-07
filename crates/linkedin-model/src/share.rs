use serde::{Deserialize, Serialize};
use strum::IntoStaticStr;

struct Share {
    author: String,
    #[rename("lifecycleState")]
    lifecycle_state: String,
    #[rename("specificContent")]
    share_content: HashMap<String, ShareContent>,
    visibility: HashMap<String, String>,
}

struct ShareContent {
    #[rename("shareCommentary")]
    share_commentary: ShareCommentary,
    #[rename("shareMediaCategory")]
    share_media_category: String,
    media: Option<Vec<ShareMedia>>,
}

struct ShareCommentary {
    text: String,
}
