use rules_core::*;
use serde_json::Value;
fn cases(file: &str) -> Vec<Value> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(file);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn run(group: &str, id: &str) {
    let c = cases(&format!("{group}_cases.json"))
        .into_iter()
        .find(|c| c["id"] == id)
        .expect("unchanged handoff fixture");
    let actual = match group {
        "candidate_selection" => Value::String(
            select(&serde_json::from_value::<Vec<Candidate>>(c["candidates"].clone()).unwrap())
                .unwrap(),
        ),
        "upgrade_decision" => serde_json::to_value(upgrade(
            &serde_json::from_value(c["local"].clone()).unwrap(),
            &serde_json::from_value(c["candidate"].clone()).unwrap(),
        ))
        .unwrap(),
        "coverage" => Value::Bool(covers(
            &serde_json::from_value(c["owned"].clone()).unwrap(),
            &c["target"],
        )),
        "task_completion" => Value::Bool(task_done(
            &c["target"],
            &c["local"],
            c["completion_revision"].as_u64().unwrap(),
        )),
        "catalog_change" => Value::String(catalog_action(&c).into()),
        "deletion_safety" => Value::Bool(old_coverage_preserved(
            &serde_json::from_value(c["old_coverage"].clone()).unwrap(),
            &serde_json::from_value(c["new_coverage"].clone()).unwrap(),
            c["new_download_complete"].as_bool().unwrap(),
        )),
        "title_normalization" => Value::Bool(
            normalize_title(c["a"].as_str().unwrap()) == normalize_title(c["b"].as_str().unwrap()),
        ),
        _ => panic!("unknown group"),
    };
    let expected = if group == "title_normalization" {
        &c["expected_equal"]
    } else {
        &c["expected"]
    };
    assert_eq!(&actual, expected, "fixture {id}");
}
macro_rules! fixture {
    ($name:ident,$group:literal,$id:literal) => {
        #[test]
        fn $name() {
            run($group, $id);
        }
    };
}
fixture!(sel_001, "candidate_selection", "SEL_001");
fixture!(sel_002, "candidate_selection", "SEL_002");
fixture!(sel_003, "candidate_selection", "SEL_003");
fixture!(sel_004, "candidate_selection", "SEL_004");
fixture!(sel_005, "candidate_selection", "SEL_005");
fixture!(sel_006, "candidate_selection", "SEL_006");
fixture!(up_001, "upgrade_decision", "UP_001");
fixture!(up_002, "upgrade_decision", "UP_002");
fixture!(up_003, "upgrade_decision", "UP_003");
fixture!(up_004, "upgrade_decision", "UP_004");
fixture!(up_005, "upgrade_decision", "UP_005");
fixture!(up_006, "upgrade_decision", "UP_006");
fixture!(up_007, "upgrade_decision", "UP_007");
fixture!(up_008, "upgrade_decision", "UP_008");
fixture!(up_009, "upgrade_decision", "UP_009");
fixture!(up_010, "upgrade_decision", "UP_010");
fixture!(cov_001, "coverage", "COV_001");
fixture!(cov_002, "coverage", "COV_002");
fixture!(cov_003, "coverage", "COV_003");
fixture!(cov_004, "coverage", "COV_004");
fixture!(cov_005, "coverage", "COV_005");
fixture!(cov_006, "coverage", "COV_006");
fixture!(cov_007, "coverage", "COV_007");
fixture!(cov_008, "coverage", "COV_008");
fixture!(task_001, "task_completion", "TASK_001");
fixture!(task_002, "task_completion", "TASK_002");
fixture!(task_003, "task_completion", "TASK_003");
fixture!(cat_001, "catalog_change", "CAT_001");
fixture!(cat_002, "catalog_change", "CAT_002");
fixture!(cat_003, "catalog_change", "CAT_003");
fixture!(cat_004, "catalog_change", "CAT_004");
fixture!(cat_005, "catalog_change", "CAT_005");
fixture!(del_001, "deletion_safety", "DEL_001");
fixture!(del_002, "deletion_safety", "DEL_002");
fixture!(del_003, "deletion_safety", "DEL_003");
fixture!(norm_001, "title_normalization", "NORM_001");
fixture!(norm_002, "title_normalization", "NORM_002");
fixture!(norm_003, "title_normalization", "NORM_003");
fixture!(norm_004, "title_normalization", "NORM_004");
fixture!(norm_005, "title_normalization", "NORM_005");

#[test]
fn task_requires_target_content() {
    assert!(!task_done(
        &serde_json::json!({"revision":1,"coverage":{"range":[1,5]}}),
        &serde_json::json!({"coverage":{"episodes":[1,2,3,4]}}),
        1
    ));
}
#[test]
fn two_unknown_higher_attributes_do_not_allow_color_upgrade() {
    let old =
        serde_json::from_value(serde_json::json!({"chinese":true,"uncensored":null,"color":false}))
            .unwrap();
    let new =
        serde_json::from_value(serde_json::json!({"chinese":true,"uncensored":null,"color":true}))
            .unwrap();
    assert_eq!(upgrade(&old, &new), Upgrade::ReviewUnknown);
}
#[test]
fn unknown_collection_does_not_authorize_coverage_removal() {
    assert!(!old_coverage_preserved(
        &state_model::Coverage::default(),
        &state_model::Coverage::default(),
        true
    ));
}
