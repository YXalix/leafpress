use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentKind {
    Post,
    Page,
}

impl ContentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentKind::Post => "post",
            ContentKind::Page => "page",
        }
    }
}

impl std::str::FromStr for ContentKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "post" => Ok(ContentKind::Post),
            "page" => Ok(ContentKind::Page),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PostMeta {
    pub title: String,
    pub slug: String,
    pub date: Option<String>,
    pub tags: Vec<String>,
    pub category: Option<String>,
    /// published / draft / hidden
    pub status: String,
    pub kind: ContentKind,
}

impl PostMeta {
    pub fn is_published(&self) -> bool {
        self.status == "published"
    }

    pub fn is_draft(&self) -> bool {
        self.status == "draft"
    }

    /// Content page URL: posts at /posts/{slug}, pages at /pages/{slug}
    pub fn href(&self) -> String {
        match self.kind {
            ContentKind::Post => format!("/posts/{}", self.slug),
            ContentKind::Page => format!("/pages/{}", self.slug),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TocItem {
    /// Heading level 1..=3
    pub level: u8,
    pub text: String,
    /// Matches the <a class="anchor" id="..."> rendered by comrak header_ids
    pub id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PostView {
    pub meta: PostMeta,
    pub html: String,
    pub toc: Vec<TocItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comment {
    pub id: i64,
    pub author: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub meta: PostMeta,
    /// Context snippet around body matches (plain text, ellipses where truncated)
    pub snippet: String,
}
