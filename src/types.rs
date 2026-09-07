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

/// Content-repo git state shown in the admin panel
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitStatus {
    /// e.g. "main...origin/main"
    pub branch: String,
    pub ahead: u32,
    pub behind: u32,
    /// `git status --porcelain` entries (uncommitted/untracked files)
    pub dirty: Vec<String>,
    /// How many of the dirty entries are staged (index differs from HEAD)
    pub staged: u32,
    /// "hash subject (relative time)"
    pub last_commit: String,
    /// Server's built-in periodic pull interval (0 = off), display only
    pub pull_interval_secs: u64,
    /// Proxy applied to git network ops (config.git_proxy), display only
    pub proxy: Option<String>,
}

/// One changed file in the working tree vs HEAD
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    /// "modified" | "added" | "deleted"
    pub status: String,
    /// true when the file (or part of it) is staged in the index
    pub staged: bool,
    pub rows: Vec<DiffRow>,
    /// true when the diff was cut short by a size cap
    pub truncated: bool,
}

/// A dual-pane diff row: a hunk header, or one left/right cell pair
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DiffRow {
    /// "@@ -a,b +c,d @@" header (without the leading @@ markers stripped)
    Hunk(String),
    Line {
        left: Option<DiffCell>,
        right: Option<DiffCell>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiffCell {
    pub no: u32,
    pub text: String,
    pub kind: DiffLineKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffLineKind {
    Context,
    Del,
    Add,
}

/// Which operation the admin git panel triggers
#[derive(Clone, Copy)]
pub enum GitOp {
    Pull,
    Push,
}
