use std::collections::HashMap;
use std::error::Error;

use rusoto_core::credential::{ChainProvider, ProvideAwsCredentials};
use rusoto_core::param::{Params, ServiceParams};
use rusoto_core::Region;
use rusoto_signature::SignedRequest;
use serde_json::json;

/// The authentication options to be passed into the main auth function
#[derive(Debug)]
pub struct Parameters {
    /// Optionally defined Vault IAM Server ID value to be attached
    /// as a header to the authentication request
    pub iam_server_id: Option<String>,
    /// The mount path of the AWS authentication engine in Vault
    pub mount_path: String,
    /// The role in Vault to authenticate as under the AWS engine
    pub role: String,
    /// The full Vault server address and port to send the request
    pub vault_address: String,
}

/// Builds the authentication request payload from the credentials
/// found in the provider chain and sends it to the designated
/// Vault server to attempt a login for the argued role
pub async fn authenticate(params: &Parameters) -> Result<serde_json::Value, Box<dyn Error>> {
    let payload = new_iam_payload(&params.role, &params.iam_server_id).await?;
    let url = format!(
        "{}/v1/auth/{}/login",
        params.vault_address, params.mount_path
    );
    let client = reqwest::Client::new();
    let res = client
        .post(url)
        .header("Accept", "application/json")
        .json(&payload)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    Ok(res)
}

/// Creates the AWS4 signed request headers and the authentication
/// payload that will be sent to Vault in the login attempt
async fn new_iam_payload(
    role: &str,
    iam_server_id: &Option<String>,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let credentials = ChainProvider::new().credentials().await?;
    let signed_request = {
        let mut req = SignedRequest::new("POST", "sts", &Region::UsEast1, "/");

        if let Some(id) = iam_server_id {
            req.add_header("X-Vault-AWS-IAM-Server-ID", id);
        }

        let mut params = Params::new();
        params.put("Action", "GetCallerIdentity");
        params.put("Version", "2011-06-15");

        req.set_payload(Some(serde_urlencoded::to_string(&params)?));
        req.set_content_type(String::from("application/x-www-form-urlencoded"));
        req.sign(&credentials);
        req
    };

    let signed_headers = {
        let mut headers = HashMap::<String, Vec<String>>::new();
        for (key, values) in signed_request.headers() {
            let entries = values
                .iter()
                .map(|v| String::from_utf8(v.to_owned()).unwrap())
                .collect::<Vec<String>>();
            headers.insert(key.to_owned(), entries);
        }
        serde_json::to_string(&headers)?
    };

    Ok(json!({
        "iam_http_request_method": "POST",
        "iam_request_url": base64::encode(b"https://sts.amazonaws.com/"),
        "iam_request_headers": base64::encode(signed_headers.as_bytes()),
        "iam_request_body": base64::encode(b"Action=GetCallerIdentity&Version=2011-06-15"),
        "role": role
    }))
}
