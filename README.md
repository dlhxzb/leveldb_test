# LevelDB 初探
打算做个LevelDBOrm crate，提供过程宏，实现orm机能
e.g.
```rust
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[derive(LevelDBOrm)]
#[level_db_key(executable,args)]
struct Command {
    pub executable: u8,
    pub args: Vec<String>,
    pub current_dir: Option<String>,
}
```