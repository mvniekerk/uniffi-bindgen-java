use std::io::BufRead;
use std::process::{Child, Command, Stdio};
use anyhow::Context;
use std::net::TcpListener;
use std::thread::JoinHandle;

#[test]
pub fn verify_native_graalvm_quarkus_native() {
    build_quarkus_linux_executable().expect("Failed to build native executable");
    let digest = get_docker_image_digest();
    let port = next_open_port();

    run_quarkus_container(&digest, port);
    get_true_or_false_value_from_http_endpoint(port);
    stop_quarkus_container(&digest);
}

fn run_quarkus_container(digest: &str, port: u16) {
    // Run quarkus container with digest, mounting port 8080 to $port, using the digest as its container name
    let _child = Command::new("docker")
        .args(&[
            "run",
            "--rm",
            "-p",
            &format!("{}:8080", port),
            "--name",
            digest,
            digest,
        ])
        .spawn()
        .expect("Failed to run quarkus container");
}

fn get_true_or_false_value_from_http_endpoint(port: u16) {
    let url = format!("http://localhost:{}/trueOrNot=true", port);
    let client = reqwest::blocking::Client::new();
    let response = client.get(&url).send().expect("Failed to send GET request");
    assert!(response.status().is_success());
    let body = response.text().expect("Failed to get response body");
    assert_eq!(body, "true");

    let url = format!("http://localhost:{}/trueOrNot=false", port);
    let client = reqwest::blocking::Client::new();
    let response = client.get(&url).send().expect("Failed to send GET request");
    assert!(response.status().is_success());
    let body = response.text().expect("Failed to get response body");
    assert_eq!(body, "false");
}

fn stop_quarkus_container(digest: &str) {
    // Stop the running quarkus container. The image digest was used as its container name
    let _child = Command::new("docker")
        .args(&["stop", digest])
        .spawn()
        .expect("Failed to stop quarkus container");
}

/// Get the next available port
fn next_open_port() -> u16 {
    // Try to bind to port 0 to let the OS assign an available port
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to an available port");
    // Get the port number
    let port = listener.local_addr().expect("Failed to get local address").port();
    // The listener is dropped here, which frees the port
    port
}

fn get_docker_image_digest() -> String {
    // File is in tests/quarkus_native/scripts/service/target/jib-image.id
    let path = "tests/quarkus_native/scripts/service/target/jib-image.id";
    let contents = std::fs::read_to_string(path)
        .expect("Failed to read jib-image.id file");
    let digest = contents.trim();
    eprintln!("Docker image digest: {}", digest);

    // It is in format sha256:<digest>, get the <digest> part
    let digest = digest.split(':').nth(1).expect("Invalid digest format");
    digest.to_string()
}

fn build_quarkus_linux_executable() -> anyhow::Result<()> {
    let has_docker = Command::new("docker")
        .arg("version")
        .spawn()
        .context("Failed to spawn `docker` to run Quarkus test")?
        .wait()
        .context("Failed to wait for `docker` when running Java test")?
        .success();
    if !has_docker {
        anyhow::bail!("Failure running docker help");
    }

    eprintln!("Cleaning");
    let child = Command::new("./mvnw")
        .current_dir("tests/quarkus_native/scripts")
        .args(&[
            "clean",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn mvnw command")?;

    run_child_process_output_and_wait(child)?;

    let image_platform = format!("-Dquarkus.jib.platforms={}", running_x86_or_arm64());

    eprintln!("Building");
    let child = Command::new("./mvnw")
        .current_dir("tests/quarkus_native/scripts")
        .args(&[
            "package",
            "-Pnative",
            "-Dquarkus.native.container-build=true",
            "-DskipTests=true",
            "-Dquarkus.container-image.build=true",
            &*image_platform
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn mvnw command")?;

    run_child_process_output_and_wait(child)?;

    Ok(())
}

fn running_x86_or_arm64() -> String {
    let output = Command::new("uname")
        .arg("-m")
        .output()
        .expect("Failed to execute uname command");

    let arch = String::from_utf8_lossy(&output.stdout);
    let arch = arch.trim();

    let arch = if arch == "x86_64" {
        "linux/amd64".to_string()
    } else if arch == "arm64" {
        "linux/arm64/v8".to_string()
    } else {
        panic!("Unsupported architecture: {}", arch)
    };
    arch
}

fn run_child_process_output_and_wait(mut child: Child) -> anyhow::Result<()> {
    let (stdout_handle, stderr_handle) = printout_child_stdout_and_stderr(&mut child)?;
    let status = child.wait().context("Failed to wait for mvnw command")?;
    stdout_handle.join().expect("stdout thread panicked");
    stderr_handle.join().expect("stderr thread panicked");

    if !status.success() {
        anyhow::bail!("mvnw command failed with status: {status}");
    } else {
        println!("Maven build completed successfully.");
    }
    Ok(())
}

fn printout_child_stdout_and_stderr(mut child: &mut Child) -> anyhow::Result<(JoinHandle<()>, JoinHandle<()>)> {

    // To stream output, we need to spawn threads to read stdout and stderr concurrently.
    let stdout = child.stdout.take().context("Failed to capture stdout from child process")?;
    let stderr = child.stderr.take().context("Failed to capture stderr from child process")?;

    let stdout_handle = std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines().flatten() {
            eprintln!("{line}");
        }
    });

    let stderr_handle = std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines().flatten() {
            eprintln!("{line}");
        }
    });
    Ok((stdout_handle, stderr_handle))
}