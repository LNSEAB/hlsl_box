if ($null -eq $args[1]) {
    Write-Host "invalid argument"
    exit 1
}

git checkout -b staging
cargo.exe release $args[1]
if ($?) {
    exit 1
}
git push -u origin staging
git checkout master
git branch -D staging