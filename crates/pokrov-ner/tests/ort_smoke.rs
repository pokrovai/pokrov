#[cfg(test)]
mod ort_smoke {
    #[test]
    fn session_builder_loads_onnx_runtime() {
        let result = ort::session::Session::builder();
        assert!(result.is_ok(), "Session::builder() should succeed with static linking: {:?}", result.err());
    }
}
