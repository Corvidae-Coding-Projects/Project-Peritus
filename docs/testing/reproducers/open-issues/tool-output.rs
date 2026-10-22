//! Real human-output projection API with independent saved-context and directory expectations.
use peritus_product_state::tool_output;
use serde_json::json;
fn main() {
    let mut correct=0;
    let mut cases=0;
    for kind in ["observation","assertion","hypothesis","decision","failed_approach","plan","glossary"] {
        for status in ["open","contradicted","resolved","stale","superseded"] {
            for text in ["Saved context body ORACLE-19273", "Unicode café 🦀\nsecond line", "", "literal ? and <tag> remain data"] {
                let value=json!({"base_revision":9,"generation":12,"authority":"none","cursor":null,"entries":[{"id":"entry:11111111111111111111111111111111","kind":kind,"status":status,"text":text,"supports":[],"contradicts":[],"depends_on":[],"supersedes":null,"authority":"none"}]});
                let before=value.clone();
                let shown=tool_output::render(&value);
                assert_eq!(value,before);
                cases+=1;
                if shown.contains(text) && shown.contains(kind) && shown.contains(status) && shown.contains("entry:11111111111111111111111111111111") {correct+=1;}
            }
        }
    }
    let empty=tool_output::render(&json!({"base_revision":1,"generation":2,"authority":"none","entries":[]}));
    let directory=tool_output::render(&json!({"path":".","entries":[{"path":"src","kind":"directory"},{"path":"readme.txt","kind":"file"},"plain.txt"]}));
    assert_eq!(directory,"src/\nreadme.txt\nplain.txt\n");
    let fragment=tool_output::render(&json!({"base_revision":1,"authority":"none","entries":[],"entry_json_fragment":{"data":"{\"text\":\"ENTRY-FRAGMENT-ORACLE"},"cursor":"opaque-next"}));
    let unknown=tool_output::render(&json!({"entries":[{"id":"entry:next","label":"updated"},17]}));
    println!("{}",json!({"context_cases":cases,"correct":correct,"empty":empty,"fragment_visible":fragment.contains("ENTRY-FRAGMENT-ORACLE"),"unknown_visible":unknown.contains("entry:next")&&unknown.contains("17"),"directory_unchanged":true}));
    if std::env::args().any(|arg|arg=="--require-fixed") {
        assert_eq!(correct,cases);
        assert!(empty.contains("No saved context"));
        assert!(fragment.contains("ENTRY-FRAGMENT-ORACLE")&&fragment.contains("partial page"));
        assert!(unknown.contains("entry:next")&&unknown.contains("17"));
    }
}
