use super::*;
use azure_devops_rust_api::git::models::git_pull_request::Status;
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};

fn repo_client(endpoint: reqwest::Url) -> RepoClient {
    let credential = Credential::from_pat("test-pat");
    RepoClient {
        core_client: core::ClientBuilder::new(credential.clone()).build(),
        git_client: git::ClientBuilder::new(credential.clone()).build(),
        work_item_client: wit::ClientBuilder::new(credential.clone())
            .endpoint(endpoint.clone())
            .build(),
        work_client: work::ClientBuilder::new(credential.clone()).build(),
        graph_client: graph::ClientBuilder::new(credential).build(),
        http_client: reqwest::Client::new(),
        http_pipeline: azure_core::http::Pipeline::new(
            Some("az-devops"),
            Some("0.1.0"),
            azure_core::http::ClientOptions::default(),
            Vec::new(),
            Vec::new(),
            None,
        ),
        http_endpoint: endpoint,
        organization: "test-org".to_string(),
        project: "Project with spaces".to_string(),
        project_id: "project-id".to_string(),
        repo_name: "test-repo".to_string(),
        repo_id: "repo-id".to_string(),
        pat: "test-pat".to_string(),
    }
}

async fn serve(responses: Vec<String>) -> (reqwest::Url, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint =
        reqwest::Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for response in responses {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let read = stream.read(&mut buffer).await.unwrap();
                assert_ne!(read, 0, "HTTP client closed before sending request headers");
                request.extend_from_slice(&buffer[..read]);
            }
            requests.push(String::from_utf8(request).unwrap());
            stream.write_all(response.as_bytes()).await.unwrap();
        }
        requests
    });
    (endpoint, task)
}

fn json_response(value: serde_json::Value) -> String {
    let body = value.to_string();
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn pull_request(id: i32, status: &str) -> serde_json::Value {
    json!({
        "pullRequestId": id,
        "title": format!("Pull request {id}"),
        "status": status,
        "createdBy": {
            "id": "author-id",
            "displayName": "Test Author",
            "uniqueName": "author@example.com",
            "_links": {}
        },
        "creationDate": "2026-10-01T12:00:00Z",
        "closedDate": if status == "active" { None } else { Some("2026-10-02T12:00:00Z") },
        "isDraft": false,
        "repository": {
            "id": "repo-id",
            "name": "test-repo",
            "project": { "name": "Project with spaces", "visibility": "private" },
            "url": "https://example.com/repository"
        },
        "sourceRefName": "refs/heads/feature",
        "targetRefName": "refs/heads/main",
        "url": format!("https://example.com/pullrequest/{id}")
    })
}

fn request_url(request: &str) -> reqwest::Url {
    let mut parts = request.lines().next().unwrap().split_whitespace();
    assert_eq!(parts.next(), Some("GET"));
    reqwest::Url::parse(&format!("http://localhost{}", parts.next().unwrap())).unwrap()
}

#[tokio::test]
async fn historical_pull_requests_keep_all_statuses_pagination_and_limit() {
    let first_page: Vec<_> = (1..=50)
        .map(|id| pull_request(id, if id % 2 == 0 { "completed" } else { "active" }))
        .collect();
    let (endpoint, server) = serve(vec![
        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nretry-after-ms: 1\r\nConnection: close\r\n\r\n".to_string(),
        json_response(json!({ "value": first_page })),
        json_response(
            json!({ "value": [pull_request(51, "abandoned"), pull_request(52, "completed")] }),
        ),
    ])
    .await;
    let client = repo_client(endpoint);
    let pull_requests = tokio::time::timeout(
        Duration::from_secs(5),
        client.get_all_pull_requests(Some(51)),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(pull_requests.len(), 51);
    assert_eq!(pull_requests[0].status, Status::Active);
    assert_eq!(pull_requests[1].status, Status::Completed);
    assert_eq!(pull_requests[50].status, Status::Abandoned);
    assert!(pull_requests[1].closed_at.is_some());
    assert_eq!(
        pull_requests.iter().map(|pr| pr.id).collect::<Vec<_>>(),
        (1..=51).collect::<Vec<_>>()
    );

    let requests = tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(requests.len(), 3);
    for (request, skip) in requests.iter().zip(["0", "0", "50"]) {
        let url = request_url(request);
        assert_eq!(
            url.path(),
            "/test-org/Project%20with%20spaces/_apis/git/repositories/repo-id/pullrequests"
        );
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query["searchCriteria.status"], "all");
        assert_eq!(query["$skip"], skip);
        assert_eq!(query["$top"], "50");
        assert_eq!(query["api-version"], "7.1-preview");
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: basic onrlc3qtcgf0\r\n"));
    }
}

#[tokio::test]
async fn attachment_download_enforces_size_before_and_during_transfer() {
    // The declared-length response deliberately has no body: checking only
    // after buffering would report a body-read error instead of PayloadTooLarge.
    let cases = [
        ("Content-Length: 1024\r\n\r\n", 6, Some(1024)),
        (
            "Transfer-Encoding: chunked\r\n\r\n4\r\n1234\r\n4\r\n5678\r\n0\r\n\r\n",
            6,
            Some(8),
        ),
        (
            "Transfer-Encoding: chunked\r\n\r\n4\r\n1234\r\n4\r\n5678\r\n0\r\n\r\n",
            8,
            None,
        ),
    ];
    for (body, max_bytes, expected_actual_bytes) in cases {
        let response =
            format!("HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nConnection: close\r\n{body}");
        let (endpoint, server) = serve(vec![response]).await;
        let client = repo_client(endpoint);
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            client.get_work_item_attachment(
                "attachment-id",
                Some("image name.png"),
                Some(max_bytes),
            ),
        )
        .await
        .unwrap();
        match expected_actual_bytes {
            Some(expected) => assert!(matches!(
                result,
                Err(RepoClientError::PayloadTooLarge { actual_bytes, max_bytes: actual_max })
                    if actual_bytes == expected && actual_max == max_bytes
            )),
            None => {
                let (bytes, content_type) = result.unwrap();
                assert_eq!(bytes, b"12345678");
                assert_eq!(content_type.as_deref(), Some("image/png"));
            }
        }
        let request = tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .pop()
            .unwrap();
        let url = request_url(&request);
        assert_eq!(
            url.path(),
            "/test-org/Project%20with%20spaces/_apis/wit/attachments/attachment-id"
        );
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query["download"], "true");
        assert_eq!(query["fileName"], "image name.png");
    }
}

#[tokio::test]
async fn work_item_comments_parse_actual_envelope_without_url() {
    let empty = json!({"totalCount": 0, "count": 0, "comments": []});
    let populated = json!({
        "totalCount": 1,
        "count": 1,
        "comments": [{
            "url": "https://example.com/comments/7",
            "id": 7,
            "text": "<p>Reviewed</p>",
            "createdBy": {"id": "author-id", "displayName": "Test Author"},
            "createdDate": "2026-10-01T12:00:00Z",
            "isDeleted": false
        }]
    });
    let (endpoint, server) = serve(vec![json_response(empty), json_response(populated)]).await;
    let client = repo_client(endpoint);
    let comments =
        tokio::time::timeout(Duration::from_secs(5), client.get_work_item_comments(1532))
            .await
            .unwrap()
            .unwrap();
    assert!(comments.is_empty());
    let comments =
        tokio::time::timeout(Duration::from_secs(5), client.get_work_item_comments(1532))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].id, 7);
    assert_eq!(comments[0].text, "<p>Reviewed</p>");
    assert_eq!(comments[0].author_name, "Test Author");
    assert_eq!(
        comments[0].created_at,
        parse_date_string("2026-10-01T12:00:00Z").unwrap()
    );
    assert!(!comments[0].is_deleted);
    let requests = tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
    for request in requests {
        assert_eq!(
            request_url(&request).path().trim_start_matches('/'),
            "test-org/Project%20with%20spaces/_apis/wit/workItems/1532/comments"
        );
    }
}

#[tokio::test]
async fn work_item_comments_reject_malformed_data_and_http_errors() {
    let forbidden_body = r#"{"message":"Forbidden"}"#;
    let responses = vec![
        json_response(json!({})),
        json_response(json!({"comments": "invalid"})),
        json_response(json!({"comments": [{"url": "https://example.com/comments/7", "id": "invalid"}]})),
        format!("HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{forbidden_body}", forbidden_body.len()),
    ];
    let (endpoint, server) = serve(responses).await;
    let client = repo_client(endpoint);
    for _ in 0..4 {
        let result =
            tokio::time::timeout(Duration::from_secs(5), client.get_work_item_comments(1532))
                .await
                .unwrap();
        assert!(matches!(result, Err(RepoClientError::AzureDevOpsError(_))));
    }
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .len(),
        4
    );
}
