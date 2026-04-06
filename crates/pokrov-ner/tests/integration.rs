use pokrov_ner::{model::NerModelBinding, NerConfig, NerEngine, NerEntityType, NerHit};

fn engine_for_languages(required_languages: &[&str]) -> Option<NerEngine> {
    let default_config = NerConfig::default();
    let mut selected_models: Vec<NerModelBinding> = Vec::new();

    for &language in required_languages {
        let Some(binding) = default_config.models.iter().find(|binding| binding.language == language)
        else {
            eprintln!("Skipping NER test: language '{language}' is not configured");
            return None;
        };
        if !binding.model_path.exists() || !binding.tokenizer_path.exists() {
            eprintln!(
                "Skipping NER test: model assets for '{language}' are missing (model='{}', tokenizer='{}')",
                binding.model_path.display(),
                binding.tokenizer_path.display()
            );
            return None;
        }
        selected_models.push(binding.clone());
    }

    let config = NerConfig { models: selected_models, ..default_config };
    NerEngine::new(config).ok()
}

#[test]
fn recognize_russian_person() {
    let Some(mut engine) = engine_for_languages(&["ru"]) else {
        return;
    };
    let hits = engine
        .recognize(
            "Меня зовут Иван Петров, я работаю в Газпроме",
            &[NerEntityType::Person, NerEntityType::Organization],
        )
        .expect("recognition must succeed");

    assert!(!hits.is_empty(), "should detect at least one entity");

    let persons: Vec<&NerHit> = hits.iter().filter(|h| h.entity == NerEntityType::Person).collect();
    assert!(!persons.is_empty(), "should detect at least one person");

    let orgs: Vec<&NerHit> =
        hits.iter().filter(|h| h.entity == NerEntityType::Organization).collect();
    assert!(!orgs.is_empty(), "should detect at least one organization");
}

#[test]
fn recognize_english_person() {
    let Some(mut engine) = engine_for_languages(&["en"]) else {
        return;
    };
    let hits = engine
        .recognize(
            "My name is John Smith and I work at Microsoft",
            &[NerEntityType::Person, NerEntityType::Organization],
        )
        .expect("recognition must succeed");

    assert!(!hits.is_empty(), "should detect at least one entity");
}

#[test]
fn empty_input_returns_empty() {
    let Some(mut engine) = engine_for_languages(&["en"]) else {
        return;
    };
    let hits =
        engine.recognize("", &[NerEntityType::Person]).expect("must not fail on empty input");
    assert!(hits.is_empty());
}

#[test]
fn latency_under_100ms() {
    let Some(mut engine) = engine_for_languages(&["en"]) else {
        return;
    };
    let text = "Contact Alice Johnson at acme@example.com for details about the project.";

    let start = std::time::Instant::now();
    for _ in 0..10 {
        let _ = engine.recognize(text, &[NerEntityType::Person, NerEntityType::Organization]);
    }
    let avg = start.elapsed() / 10;
    assert!(avg.as_millis() < 100, "avg latency {:?} exceeds 100ms budget", avg);
}
