$target_dir = $PSScriptRoot + "/../target/dxc"
$create_at_file = $target_dir + "/create_at.txt"
$uri = "https://api.github.com/repos/microsoft/DirectXShaderCompiler/releases/latest"
$date_format = "yyyyMMddhhmmss";

if ((Test-Path -Path $target_dir) -and !($args.Contains("--update"))) {
    exit 0
}

Write-Host "[dxc.ps1] start"

$response = Invoke-WebRequest -Uri $uri
if ($response.StatusCode -ne 200) {
    Write-Host "[dxc.ps1] cannot get the release info"
    exit 1
}
$data = ConvertFrom-Json $response.Content
foreach ($asset in $data.assets) {
    if ($asset.name -match "dxc_[a-zA-Z0-9_-]+\.zip") {
        if (Test-Path -Path $create_at_file) {
            $prev_created_at = [DateTime]::ParseExact((Get-Content -Raw $create_at_file), $date_format, $null)
            if (($asset.created_at - $prev_created_at).Seconds -eq 0) {
                Write-Host "[dxc.ps1] not modified"
                exit 0
            }
        }
        $response_zip = Invoke-WebRequest -Uri $asset.browser_download_url
        if ($response_zip.StatusCode -ne 200) {
            Write-Host "[dxc.ps1] cannot get the zip file"
            exit 1
        }
        if (Test-Path -Path $target_dir) {
            Remove-Item -Recurse -Force -Path $target_dir | Out-Null
        }
        New-Item $target_dir -ItemType Directory | Out-Null
        $file_path = $target_dir + "/" + $asset.name
        [System.IO.File]::WriteAllBytes($file_path, $response_zip.Content);
        Expand-Archive -Path $file_path -DestinationPath $target_dir -Force
        Write-Output $asset.created_at.ToString($date_format) | Out-File -NoNewline -FilePath $create_at_file -Encoding utf8
        break
    }
}
Write-Host "[dxc.ps1] finished"