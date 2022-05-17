git checkout -b staging
cargo.exe release $args[1] --execute
git push -u origin staging
git branch -D staging