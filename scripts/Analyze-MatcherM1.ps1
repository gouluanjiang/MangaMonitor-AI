param([switch]$Check)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$evidencePath = Join-Path $root 'evidence/phase3b-run-33998989019/review-export.json'
$inventoryPath = Join-Path $root 'monitor-state/inventory_index.json'
$review = Get-Content -LiteralPath $evidencePath -Raw | ConvertFrom-Json
$inventory = Get-Content -LiteralPath $inventoryPath -Raw | ConvertFrom-Json
function Key([string]$s) { ($s.Normalize([Text.NormalizationForm]::FormKC).ToLowerInvariant() -replace '\s+', ' ').Trim() }
function Broken([string]$s) {
    $stack = [Collections.Generic.List[char]]::new()
    foreach ($c in $s.ToCharArray()) {
        if ('([【'.Contains($c)) { $stack.Add($c) }
        elseif (')]】'.Contains($c)) {
            if (!$stack.Count) { return $true }
            $expected = switch ($c) { ')' {'('} ']' {'['} '】' {'【'} }
            if ($stack[$stack.Count - 1] -ne $expected) { return $true }
            $stack.RemoveAt($stack.Count - 1)
        }
    }
    return $stack.Count -ne 0
}
# This is a diagnostic inventory of literal features, NOT a runtime matcher.
# A regex hit is auditable text evidence, never an identity/translation assertion.
$patterns = [ordered]@{
    event_wrapper = '\((?:c\d{2,3}|comic1[☆★]?\d+)\)'
    publication_wrapper = '\(コミックホットミルク \d{4}年\d+月[号號]\)'
    circle_author_wrapper = '\[[^\[\]]*\([^()]+\)\]'
    translator_annotation = '[\[【(][^\]】)]*(?:漢化|汉化|翻譯|翻译|翻訳|機翻|机翻|translated|重嵌|潤色)[^\]】)]*[\]】)]'
    language_annotation = '[\[(](?:chinese|中文|中国语|中譯|中[国國文](?:翻[译譯訳]))[\])]'
    release_version_annotation = '\[(?:dl版|digital|無修正|无修正|全彩|黑白)\]'
    bilingual_separator = '[|丨]'
    numeric_lexeme_raw_including_metadata = '\d+'
    volume_lexeme = '(?:第\s*\d+\s*[卷巻]|\bvol(?:ume)?\.?\s*\d+)'
    episode_chapter_lexeme = '(?:第\s*\d+\s*[话話]|\b(?:episode|chapter|ch\.)\s*\d+)'
    part_lexeme = '\bpart\s*\d+'
    front_back_lexeme = '前[篇編]|[后後][篇編]'
    upper_lower_lexeme = '(?:^|\s|[\[【(])[上下](?:[篇編卷巻]|$|\s|[\]】)])'
    range_lexeme = '\d+\s*[-–—~〜～]\s*\d+'
    extra_bonus_after_story_lexeme = '\bextra\b|\bbonus\b|after[- ]story|おまけ|番外|特典|後日談|后日谈|外傳|外伝'
    collection_lexeme = '総集[篇編]|[总總]集[篇編]|合集|collection|オムニバス'
    cg_lexeme = 'cg(?:集|图集|圖集| set| collection)'
    artbook_lexeme = '画集|畫集|イラスト集|artbook'
    novel_lexeme = '小说|小說|小説|\bnovel\b'
    settings_lexeme = '设定集|設定集|設定資料|\bsettings\b|setting book'
    artist_only_low_information = '^\[artist\]\s*haruhisky$'
}
# Manually inspected local representation differences. These are review leads,
# not approved aliases. Local/source strings stay untouched in the output.
$leads = @{
    'jm:1207476' = @('WORK_00001', 'GLYPH_编_編_AND_FANDOM')
    'pica:68a74893dbeebf5c23e27386' = @('WORK_00001', 'GLYPH_编_編_AND_FANDOM')
    'jm:1244034' = @('WORK_02583', 'GLYPH_溫_温_AND_LOCAL_WRAPPERS')
    'jm:1247353' = @('WORK_02583', 'GLYPH_溫_温_AND_LOCAL_WRAPPERS')
    'pica:695a895aefd3a177b41c44de' = @('WORK_02583', 'LOCAL_WRAPPERS_AND_FANDOM')
    'jm:616277' = @('WORK_01173', 'BILINGUAL_LOCAL_PRIMARY')
    'jm:1216029' = @('WORK_01173', 'BILINGUAL_LOCAL_PRIMARY')
    'jm:1216028' = @('WORK_01173', 'BILINGUAL_LOCAL_PRIMARY')
    'pica:66be18bc9d3c3a1b488e8b8e' = @('WORK_01173', 'BILINGUAL_SEPARATOR_丨_PIPE')
    'pica:68e4ff0640524f74e62f2c20' = @('WORK_01173', 'BILINGUAL_TRANSLATION_DIFFERENCE')
}
$rows = @($review.items | Where-Object reason -eq 'UNRESOLVED_TITLE_IDENTITY' | Sort-Object source_key | ForEach-Object {
    $item = $_; $raw = [string]$item.raw_title; $key = Key $raw
    $locals = @($inventory.works | Where-Object { $_.work_id -cin $item.candidate_work_ids })
    if ($locals.Count -ne $item.candidate_work_ids.Count) { throw "Missing candidate: $($item.source_key)" }
    $titles = @($locals | ForEach-Object { $w = $_; foreach ($t in $w.title_candidates) {
        [pscustomobject][ordered]@{work_id=$w.work_id; primary=$t.primary; fandom_or_source=$t.fandom_or_source}
    } })
    $hits = [ordered]@{}
    foreach ($p in $patterns.GetEnumerator()) {
        $matches = @([regex]::Matches($key, $p.Value) | ForEach-Object Value | Sort-Object -Unique)
        if ($matches.Count) { $hits[$p.Key] = $matches }
    }
    if (![string]::Equals($raw, $raw.Normalize([Text.NormalizationForm]::FormKC), [StringComparison]::Ordinal)) { $hits.nfkc_changes = @('raw != NFKC(raw)') }
    if (![string]::Equals($raw, $raw.ToLowerInvariant(), [StringComparison]::Ordinal)) { $hits.case_changes = @('raw != lowercase(raw); includes wrappers') }
    if (Broken $key) { $hits.malformed_brackets = @('unbalanced or mismatched bracket stack') }
    $literal = @($titles | Where-Object { $null -ne $_.primary -and (Key $_.primary) -ne '' -and $key.Contains((Key $_.primary)) } | ForEach-Object work_id)
    $lead = $leads[$item.source_key]
    $group = if (!@($titles | Where-Object { $null -ne $_.primary -and (Key $_.primary) -ne '' }).Count) { 'NULL_LOCAL_PRIMARY' }
        elseif ($hits.Contains('malformed_brackets')) { 'MALFORMED_BRACKETS' }
        elseif (@('range_lexeme','extra_bonus_after_story_lexeme','collection_lexeme','cg_lexeme','artbook_lexeme','novel_lexeme','settings_lexeme','artist_only_low_information') | Where-Object { $hits.Contains($_) }) { 'IDENTITY_SENSITIVE_OR_LOW_INFORMATION' }
        elseif ($literal.Count) { 'LOCAL_PRIMARY_LITERAL_WITH_RESIDUAL' }
        elseif ($null -ne $lead) { 'LOCAL_REPRESENTATION_DIFFERENCE' }
        else { 'NO_LITERAL_LOCAL_PRIMARY_WITNESS' }
    [pscustomobject][ordered]@{
        source_key=$item.source_key; review_id=$item.review_id; author=$item.author_evidence.canonical_author
        raw_title=$raw; primary_group=$group; features=$hits; candidate_titles=$titles
        literal_local_work_ids=$literal
        manually_reviewed_lead=$(if ($null -ne $lead) { [ordered]@{work_id=$lead[0]; reason=$lead[1]} } else {$null})
    }
})
if ($rows.Count -ne 181) { throw 'Expected exactly 181 unresolved records' }
$groups = @($rows | Group-Object primary_group | Sort-Object Name | ForEach-Object { [pscustomobject][ordered]@{
    group=$_.Name; count=$_.Count
    jm=@($_.Group | Where-Object source_key -like 'jm:*').Count
    pica=@($_.Group | Where-Object source_key -like 'pica:*').Count
} })
$features = @($patterns.Keys) + @('nfkc_changes','case_changes','malformed_brackets') | ForEach-Object {
    $name=$_; [pscustomobject][ordered]@{feature=$name; count=@($rows | Where-Object { $_.features.Contains($name) }).Count}
}
$output = [ordered]@{
    schema_version=1; baseline='86b8e13ea856cc8e707c071841ee8da8486c1fb3'
    review_sha256=(Get-FileHash -LiteralPath $evidencePath -Algorithm SHA256).Hash.ToLowerInvariant()
    inventory_sha256=(Get-FileHash -LiteralPath $inventoryPath -Algorithm SHA256).Hash.ToLowerInvariant()
    count=$rows.Count; groups=$groups; overlapping_features=@($features); items=$rows
}
$json = ($output | ConvertTo-Json -Depth 25) + "`n"
$destination = Join-Path $root 'fixtures/matcher-m1/evidence-analysis.json'
if ($Check) {
    $existing=Get-Content -LiteralPath $destination -Raw | ConvertFrom-Json | ConvertTo-Json -Depth 25 -Compress
    $current=$output | ConvertTo-Json -Depth 25 -Compress
    if (![string]::Equals($existing, $current, [StringComparison]::Ordinal)) { throw 'M1 evidence analysis drift' }
    Write-Output 'M1 evidence analysis: all 181 rows and source hashes verified'
} else {
    [IO.Directory]::CreateDirectory((Split-Path -Parent $destination)) | Out-Null
    [IO.File]::WriteAllText($destination, $json, [Text.UTF8Encoding]::new($false))
    $groups | Format-Table
    $features | Format-Table
}
