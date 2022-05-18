if ($null -eq $Args[0]) {
    Write-Host "invalid argument"
    exit 1
}

$branch = "staging"
$prev_branch = git branch --contains | ForEach-Object {$($_ -split(" "))[1]}

git checkout -b $branch
cargo.exe release $Args[0] --execute
if (!$?) {
    git checkout Cargo.lock
    git checkout Cargo.toml
    git checkout $prev_branch 
    git branch -d $branch
    exit 1
}
git push -u origin $branch
git checkout $prev_branch
git branch -D $branch