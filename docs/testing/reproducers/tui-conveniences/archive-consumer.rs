#[path = "../../../../crates/app/peritus-tui/src/transcript_archive.rs"]
mod transcript_archive;

use peritus_app_protocol::{ProductActivity, ProductActivityKind};
use std::{fs, path::PathBuf};

const SCOPE: &str = "0123456789abcdef0123456789abcdef";

fn main() -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(run())
}

async fn run() -> Result<(), String> {
    if std::env::args().nth(1).as_deref() == Some("restart") {
        let page = transcript_archive::search(SCOPE, "unique-needle-1123").await?;
        assert_eq!(page.hits.len(), 1);
        assert_eq!(page.hits[0].sequence, 1123);
        assert_eq!(page.first_sequence, Some(1000));
        assert_eq!(
            transcript_archive::search(SCOPE, "fold σsk").await?.hits[0].sequence,
            1043
        );
        println!("PASS restart cache, captured-gap metadata, Unicode search (rg or Rust fallback)");
        return Ok(());
    }
    let mut activities = Vec::new();
    for sequence in 1000..1300 {
        let kind = match sequence % 5 {
            0 => ProductActivityKind::User,
            1 => ProductActivityKind::Assistant,
            2 => ProductActivityKind::Tool,
            3 => ProductActivityKind::Status,
            _ => ProductActivityKind::Error,
        };
        let special = match sequence {
            1005 => "stream-old".to_owned(),
            1042 => "literal [abc].* --flag".to_owned(),
            1043 => "fold ΣſK".to_owned(),
            1044 => "line one\nline two".to_owned(),
            1045 => "before\0after\x1b[31m".to_owned(),
            1046 => "--help".to_owned(),
            1047 => "é".repeat(4000),
            _ => format!("unique-needle-{sequence}"),
        };
        activities.push(
            ProductActivity::new(
                sequence,
                kind,
                format!("Title {sequence}"),
                format!("shared-token {special}"),
            )
            .map_err(|error| format!("{error:?}"))?,
        );
    }
    transcript_archive::record(SCOPE, &activities)?;
    let cache = PathBuf::from(std::env::var_os("XDG_CACHE_HOME").expect("isolated cache"))
        .join("peritus/tui-transcripts")
        .join(SCOPE);
    assert_eq!(activity_count(&cache), 300);
    let common = transcript_archive::search(SCOPE, "shared-token").await?;
    assert_eq!(common.hits.len(), 200);
    assert!(common.truncated);
    assert_eq!(common.first_sequence, Some(1000));
    for (query, sequence) in [
        ("unique-needle-1123", 1123),
        ("[abc].*", 1042),
        ("fold σsk", 1043),
        ("line one\nline two", 1044),
        ("\0", 1045),
        ("--help", 1046),
    ] {
        let page = transcript_archive::search(SCOPE, query).await?;
        assert_eq!(page.hits.len(), 1, "query {query:?}");
        assert_eq!(page.hits[0].sequence, sequence, "query {query:?}");
        assert!(!page.truncated);
    }
    let control = transcript_archive::search(SCOPE, "\0").await?;
    assert_eq!(
        control.hits[0].body,
        "Title 1045\nshared-token before\0after\x1b[31m"
    );
    let full = transcript_archive::search(SCOPE, "ééééé").await?;
    assert_eq!(
        full.hits[0].body,
        format!("Title 1047\nshared-token {}", "é".repeat(4000))
    );
    let path = cache.join("00000000000000001005.activity");
    let before = fs::metadata(&path).unwrap().modified().unwrap();
    let first_before = fs::metadata(cache.join("first-sequence"))
        .unwrap()
        .modified()
        .unwrap();
    transcript_archive::record(SCOPE, &activities)?;
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    assert_eq!(
        fs::metadata(cache.join("first-sequence"))
            .unwrap()
            .modified()
            .unwrap(),
        first_before
    );
    let updated = ProductActivity::new(
        1005,
        ProductActivityKind::User,
        "Title 1005".to_owned(),
        "shared-token stream-new".to_owned(),
    )
    .map_err(|error| format!("{error:?}"))?;
    transcript_archive::record(SCOPE, &[updated])?;
    assert!(
        transcript_archive::search(SCOPE, "stream-old")
            .await?
            .hits
            .is_empty()
    );
    assert_eq!(
        transcript_archive::search(SCOPE, "stream-new")
            .await?
            .hits
            .len(),
        1
    );
    assert_eq!(activity_count(&cache), 300);
    let isolated =
        transcript_archive::search("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "shared-token").await?;
    assert!(isolated.hits.is_empty());
    assert_eq!(isolated.first_sequence, None);
    assert!(transcript_archive::search("../escape", "x").await.is_err());
    assert!(transcript_archive::search(SCOPE, "").await.is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt as _, symlink};
        assert_eq!(fs::metadata(&cache).unwrap().mode() & 0o777, 0o700);
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        let linked = cache
            .parent()
            .unwrap()
            .join("cccccccccccccccccccccccccccccccc");
        symlink(&cache, &linked).unwrap();
        assert!(
            transcript_archive::search("cccccccccccccccccccccccccccccccc", "shared-token")
                .await
                .is_err()
        );
        fs::remove_file(linked).unwrap();
    }
    println!(
        "PASS 300 public activities; all five kinds; 200-hit truncation; literal/Unicode/multiline/NUL search; exact large/control-bearing bodies; no-op mtimes; atomic streaming replacement; run isolation; scope and symlink rejection; private modes"
    );
    Ok(())
}

fn activity_count(directory: &std::path::Path) -> usize {
    fs::read_dir(directory)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".activity"))
        .count()
}
