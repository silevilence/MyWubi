use core_engine::{
    config::{ModelLoading, ModelSelection},
    Config,
};

#[test]
fn saving_and_reopening_preserves_prediction_choices_without_installing_a_model() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    let table = directory.path().join("fixture.dict");
    std::fs::write(&table, "b\t北\nbj\t北京\n").unwrap();
    let mut config = Config::default();
    config.dictionary.system_table = table;
    config.dictionary.enable_user_dict = false;
    config.save(&path).unwrap();
    let mut state = settings::state::AppState::load(path.clone(), true);
    assert!(!state.config.prediction.enabled);
    state.config.prediction.wait_ms = 850;
    state.config.prediction.loading = ModelLoading::PreloadKeep;
    state.config.prediction.model = Some(ModelSelection {
        id: "absent-mi27".into(),
        version: "v1".into(),
    });
    assert!(settings::save::save(&mut state), "{:?}", state.status_msg);
    let mut reopened = settings::state::AppState::load(path.clone(), true);
    assert_eq!(reopened.config.prediction, state.config.prediction);
    assert!(!reopened.config.prediction.enabled);
    reopened.config.prediction.enabled = true;
    assert!(settings::save::save(&mut reopened));
    assert_eq!(
        Config::load(path).unwrap().prediction,
        reopened.config.prediction
    );
    assert!(!directory.path().join("models/absent-mi27--v1").exists());
}
