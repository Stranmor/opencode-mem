#[cfg(test)]
mod tests {
    use crate::ai_types::{ChatRequest, Message, ResponseFormat};
    use crate::client::LlmClient;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn setup_mock_server() -> MockServer {
        MockServer::start().await
    }

    fn create_test_request() -> ChatRequest {
        ChatRequest {
            model: "test-model".to_owned(),
            messages: vec![Message { role: "user".to_owned(), content: "hello".to_owned() }],
            response_format: ResponseFormat { format_type: "text".to_owned() },
        }
    }

    #[tokio::test]
    async fn test_success_on_first_attempt() {
        let server = setup_mock_server().await;
        let client = LlmClient::new("test-key".to_owned(), server.uri());
        let request = create_test_request();

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("Authorization", "Bearer test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": {
                        "content": "test response",
                        "role": "assistant"
                    }
                }]
            })))
            .mount(&server)
            .await;

        let result = client.chat_completion(&request).await.unwrap();
        assert_eq!(result, "test response");
    }

    #[tokio::test]
    async fn test_retry_on_429_then_success() {
        let server = setup_mock_server().await;
        let client = LlmClient::new("test-key".to_owned(), server.uri());
        let request = create_test_request();

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": {
                        "content": "success after retry",
                        "role": "assistant"
                    }
                }]
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(429).set_body_string("Rate limit exceeded"))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let result = client.chat_completion(&request).await.unwrap();
        assert_eq!(result, "success after retry");
    }

    #[tokio::test]
    async fn test_retry_on_503_then_success() {
        let server = setup_mock_server().await;
        let client = LlmClient::new("test-key".to_owned(), server.uri());
        let request = create_test_request();

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": {
                        "content": "success after 503",
                        "role": "assistant"
                    }
                }]
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let result = client.chat_completion(&request).await.unwrap();
        assert_eq!(result, "success after 503");
    }

    #[tokio::test]
    async fn test_no_retry_on_401() {
        let server = setup_mock_server().await;
        let client = LlmClient::new("test-key".to_owned(), server.uri());
        let request = create_test_request();

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .expect(1)
            .mount(&server)
            .await;

        let result = client.chat_completion(&request).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("401"));
        assert!(err_msg.contains("Unauthorized"));
    }

    #[tokio::test]
    async fn test_all_retries_exhausted() {
        let server = setup_mock_server().await;
        let client = LlmClient::new("test-key".to_owned(), server.uri());
        let request = create_test_request();

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
            .expect(4)
            .mount(&server)
            .await;

        let result = client.chat_completion(&request).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("503"));
        assert!(err_msg.contains("Service Unavailable"));
    }
}
