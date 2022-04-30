$dir = "package/hlsl_box"

if (Test-Path -Path $dir) {
    Remove-Item -Recurse -Force -Path "package" | Out-Null
    Write-Host "remove package"
}
New-Item $dir -ItemType Directory | Out-Null
Write-Host "create $dir"

cargo.exe build --profile production
cargo.exe about generate about.hbs > package/hlsl_box/license.html
Copy-Item "dxcompiler.dll" $dir
Copy-Item "dxil.dll" $dir
Copy-Item "target/production/hlsl_box.exe" $dir
Copy-Item -Recurse -Force "include" $dir 
Copy-Item -Recurse -Force "examples" $dir 

Compress-Archive -Force -Path $dir -DestinationPath "package/hlsl_box.zip"

Write-Host "finish package.ps1"
