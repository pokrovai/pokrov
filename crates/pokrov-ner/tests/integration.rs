use pokrov_ner::{model::NerModelBinding, NerConfig, NerEngine, NerEntityType, NerHit};

fn engine_from_env() -> Option<NerEngine> {
    let en_model = std::env::var("NER_MODEL_PATH").ok()?;
    let en_tokenizer = std::env::var("NER_TOKENIZER_PATH").ok()?;
    let ru_model = std::env::var("NER_RU_MODEL_PATH").ok();
    let ru_tokenizer = std::env::var("NER_RU_TOKENIZER_PATH").ok();

    let mut models = vec![NerModelBinding {
        language: "en".to_string(),
        model_path: en_model.into(),
        tokenizer_path: en_tokenizer.into(),
        priority: 100,
    }];

    if let (Some(rp), Some(rt)) = (ru_model, ru_tokenizer) {
        models.push(NerModelBinding {
            language: "ru".to_string(),
            model_path: rp.into(),
            tokenizer_path: rt.into(),
            priority: 100,
        });
    }

    let config = NerConfig { models, ..NerConfig::default() };
    NerEngine::new(config).ok()
}

#[test]
#[ignore = "requires NER_MODEL_PATH and NER_TOKENIZER_PATH env vars"]
fn recognize_russian_person() {
    let mut engine = engine_from_env().expect("NER engine must initialize");
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
#[ignore = "requires NER_MODEL_PATH and NER_TOKENIZER_PATH env vars"]
fn recognize_english_person() {
    let mut engine = engine_from_env().expect("NER engine must initialize");
    let hits = engine
        .recognize(
            "My name is John Smith and I work at Microsoft",
            &[NerEntityType::Person, NerEntityType::Organization],
        )
        .expect("recognition must succeed");

    assert!(!hits.is_empty(), "should detect at least one entity");
}

#[test]
#[ignore = "requires NER_MODEL_PATH and NER_TOKENIZER_PATH env vars"]
fn empty_input_returns_empty() {
    let mut engine = engine_from_env().expect("NER engine must initialize");
    let hits =
        engine.recognize("", &[NerEntityType::Person]).expect("must not fail on empty input");
    assert!(hits.is_empty());
}

#[test]
#[ignore = "requires NER_MODEL_PATH and NER_TOKENIZER_PATH env vars"]
fn latency_under_100ms() {
    let mut engine = engine_from_env().expect("NER engine must initialize");
    let text = "Contact Alice Johnson at acme@example.com for details about the project.";

    let start = std::time::Instant::now();
    for _ in 0..10 {
        let _ = engine.recognize(text, &[NerEntityType::Person, NerEntityType::Organization]);
    }
    let avg = start.elapsed() / 10;
    assert!(avg.as_millis() < 100, "avg latency {:?} exceeds 100ms budget", avg);
}
