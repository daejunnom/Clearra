function Invoke-ProductE2ET4GoldenCases {
    Invoke-ProductE2ECommandCase `
        -Name "T4 pc 4L bag pattern golden contract" `
        -FixturePath "tests/fixtures/product/pc_4l_bag_pattern.json" `
        -GoldenPath "tests/golden/product/pc_4l_bag_pattern.json" `
        -CommandArgs (Get-FixtureCommandArgs "tests/fixtures/product/pc_4l_bag_pattern.json")

    Invoke-ProductE2ECommandCase `
        -Name "T4 scenario clear-to-empty golden contract" `
        -FixturePath "tests/fixtures/product/scenario_clear_to_empty.json" `
        -GoldenPath "tests/golden/product/scenario_clear_to_empty.json" `
        -CommandArgs (Get-FixtureCommandArgs "tests/fixtures/product/scenario_clear_to_empty.json")

    Invoke-ProductE2ECommandCase `
        -Name "T4 percent uniform bag golden contract" `
        -FixturePath "tests/fixtures/product/percent_uniform_bag.json" `
        -GoldenPath "tests/golden/product/percent_uniform_bag.json" `
        -CommandArgs (Get-FixtureCommandArgs "tests/fixtures/product/percent_uniform_bag.json")

    Invoke-ProductE2ECommandCase `
        -Name "T4 rules verify basic golden contract" `
        -FixturePath "tests/fixtures/product/rules_verify_basic.json" `
        -GoldenPath "tests/golden/product/rules_verify_basic.json" `
        -CommandArgs (Get-FixtureCommandArgs "tests/fixtures/product/rules_verify_basic.json")
}
