git checkout -b staging
cargo.exe release $args[1]
git push -u origin staging
git branch -D staging