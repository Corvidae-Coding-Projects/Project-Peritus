//! Real OAuth and inference consumer; never fabricates a grant or substitutes provider output.
use peritus_model_protocol::*;
use peritus_provider_core::{
    CancellationToken, ModelProvider, connection::verify_provider_connection,
};
use peritus_provider_openai::{ChatGptAccount, ChatGptLogin, ChatGptProvider};
use peritus_types::ProviderProfileId;
use std::path::PathBuf;
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args()
            .nth(2)
            .ok_or("usage: oauth-consumer login|catalog|verify ABSOLUTE_SESSION_ROOT [MODEL]")?,
    );
    let cancel = CancellationToken::new();
    let action = std::env::args().nth(1).unwrap_or("login".into());
    if !["login", "catalog", "verify"].contains(&action.as_str()) {
        return Err("unknown action".into());
    }
    let account = if action == "login" {
        let login = ChatGptLogin::begin(&root, None).await?;
        println!("Continue with ChatGPT: {}", login.authorization_url());
        println!("Authorization listener ready. Waiting for browser consent.");
        login.finish(&cancel).await?
    } else {
        ChatGptAccount::list(&root)?
            .into_iter()
            .next()
            .ok_or("No saved Peritus OAuth registration")?
    };
    println!("Validated registration: authorized={}", account.authorized());
    let saved = ChatGptAccount::from_reference(&account.credential_reference())?;
    let models = saved.models(&cancel).await?;
    println!("Current catalog: {} models", models.len());
    for model in &models {
        println!("model: {}", model.id.as_str());
    }
    if action == "verify" {
        let model = std::env::args().nth(3).unwrap_or(models[0].id.as_str().to_owned());
        let profile = ProviderProfile::new(
            ProviderProfileId::new([14; 16]).map_err(|_| "profile id")?,
            1,
            ProviderName::new("openai".into())?,
            ModelName::new(model)?,
            WireDialect::OpenAiResponses,
            CapabilityMatrix::new(
                &[
                    Capability::ToolCalls,
                    Capability::Streaming,
                    Capability::UsageDetail,
                    Capability::ParallelToolCalls,
                    Capability::ReasoningControls,
                    Capability::ReasoningSummaries,
                    Capability::ReasoningReplay,
                    Capability::ImageInput,
                    Capability::PromptCaching,
                ],
                &[],
            )?,
            CapabilityProvenance::Profiled,
            ModelLimits::new(200000, 64000, 64, 8, 33554432)?,
            OutputLimitEnforcement::Advisory,
            StateMode::StatelessReplay,
            ResumeKind::Unsupported,
            CancellationKind::BestEffortLocalAbort,
        )?;
        let provider = ChatGptProvider::new(&saved.credential_reference(), profile)?;
        provider.prepare_context(&cancel).await?;
        let report = verify_provider_connection(&provider, cancel).await?;
        for stage in report.completed {
            println!("Passed: {}", stage.label());
        }
    }
    Ok(())
}
