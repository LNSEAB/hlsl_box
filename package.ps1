$dir = "package/hlsl_box"

cargo.exe build --profile production

if (Test-Path -Path $dir) {
    Remove-Item -Recurse -Force -Path $dir | Out-Null
}
New-Item $dir -ItemType Directory | Out-Null
Copy-Item "dxcompiler.dll" $dir
Copy-Item "dxil.dll" $dir
Copy-Item "target/production/hlsl_box.exe" $dir
Copy-Item -Recurse -Force "include" $dir 
Copy-Item -Recurse -Force "examples" $dir 
Compress-Archive -Force -Path $dir -DestinationPath "package/hlsl_box.zip"
Write-Host "finish package.ps1"