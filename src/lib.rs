use futures_util::AsyncWriteExt;
use nftables::{helper::NftablesError, schema::Nftables};
use process::Process;

const NFT_DEFAULT_PROGRAM: &str = "nft";

pub mod process;

pub async fn apply_ruleset<P: Process>(
    nftables: &Nftables,
    program: Option<&str>,
    args: Option<Vec<&str>>,
) -> Result<(), NftablesError> {
    let payload = serde_json::to_string(nftables).map_err(NftablesError::NftInvalidJson)?;
    apply_ruleset_raw::<P>(payload, program, args).await
}

pub async fn apply_ruleset_raw<P: Process>(
    payload: String,
    program: Option<&str>,
    args: Option<Vec<&str>>,
) -> Result<(), NftablesError> {
    let program = program.unwrap_or(NFT_DEFAULT_PROGRAM);
    let mut arg_vec = vec!["-j", "-f", "-"];

    if let Some(args) = args {
        arg_vec.extend(args);
    }

    let mut child =
        P::spawn(program, arg_vec, true).map_err(|err| NftablesError::NftExecution {
            program: program.to_owned(),
            inner: err,
        })?;

    let mut stdin = child
        .take_stdin()
        .expect("Stdin was piped to the process but could not be retrieved");
    stdin
        .write_all(payload.as_bytes())
        .await
        .map_err(|err| NftablesError::NftExecution {
            program: program.to_owned(),
            inner: err,
        })?;
    drop(stdin);

    match child.wait_with_output().await {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stdout = read(program, output.stdout)?;
            let stderr = read(program, output.stderr)?;

            Err(NftablesError::NftFailed {
                program: program.to_owned(),
                hint: "applying ruleset".to_string(),
                stdout,
                stderr,
            })
        }
        Err(err) => Err(NftablesError::NftExecution {
            program: program.to_owned(),
            inner: err,
        }),
    }
}

pub async fn get_current_ruleset<P: Process>(
    program: Option<&str>,
    args: Option<Vec<&str>>,
) -> Result<Nftables, NftablesError> {
    let output = get_current_ruleset_raw::<P>(program, args).await?;
    serde_json::from_str(&output).map_err(NftablesError::NftInvalidJson)
}

pub async fn get_current_ruleset_raw<P: Process>(
    program: Option<&str>,
    args: Option<Vec<&str>>,
) -> Result<String, NftablesError> {
    let program = program.unwrap_or(NFT_DEFAULT_PROGRAM);
    let mut arg_vec = vec!["-j", "list", "ruleset"];
    if let Some(args) = args {
        arg_vec.extend(args);
    }

    let output = P::output(program, arg_vec)
        .await
        .map_err(|err| NftablesError::NftExecution {
            program: program.to_owned(),
            inner: err,
        })?;

    let stdout = read(program, output.stdout)?;

    if !output.status.success() {
        let stderr = read(program, output.stderr)?;

        return Err(NftablesError::NftFailed {
            program: program.to_owned(),
            hint: "getting the current ruleset".to_string(),
            stdout,
            stderr,
        });
    }

    Ok(stdout)
}

#[inline]
fn read(program: &str, stream: Vec<u8>) -> Result<String, NftablesError> {
    String::from_utf8(stream).map_err(|err| NftablesError::NftOutputEncoding {
        program: program.to_owned(),
        inner: err,
    })
}
