# Four-repository workflow

Build the public crates from sibling checkouts. No private registry is required.

```text
workspace/
  bindb/
  codeintel/
  binder-llm/
  binder-ai/
  sample/          # a small git repository you create
```

Record the commits you checked out. The commands below use those working trees, not a moving branch inside the tools.

```sh
cd workspace
git clone https://github.com/dpkimball/bindb
git clone https://github.com/dpkimball/codeintel
git clone https://github.com/dpkimball/binder-llm
git clone https://github.com/dpkimball/binder-ai

# Synthetic repository. Do not point this at a private tree if the store will be shared.
mkdir sample && cd sample
git init
mkdir -p src
printf 'fn main() {\n    println!("sample");\n}\n' > src/main.rs
git add src/main.rs && git commit -m "sample"
cd ..

cd codeintel
cargo test --locked
cargo run --locked -- index ../sample --store ../store --name sample
cargo run --locked -- query --store ../store --name sample --question summary

cd ../binder-ai
cargo test --locked --all-features
cargo run --locked --bin binder-ai-plan -- \
  --store ../store \
  --workspace ../sample \
  --codeintel ../codeintel/target/debug/codeintel \
  --request "what is in src/main.rs"
```

`binder-ai-plan` reads the codeintel store and prints an inspection set. It does not call a model and it does not write files.

`binder-ai-serve` is a separate process. Without `--listen` it binds `127.0.0.1:7841` and has no authentication. Do not publish that port. `POST /plan` does not call a model. `POST /ask` and `POST /explain` can.

Delete `workspace/store` when you are done. A store made from a private repository can contain private issue and pull-request text if those were indexed.
