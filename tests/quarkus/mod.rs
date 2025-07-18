use std::io::BufRead;
use std::process::{Child, Command, Stdio};
use anyhow::Context;
use std::net::TcpListener;
use std::thread::JoinHandle;

///
/// The native build
/// 
/// Builds a runnable Docker container, starts it, and compares output from the endpoint (that
/// loads the uniffi-java-bindgen generated library).
/// This is flaky on arm64 as of 2025-07-17
/// It should be improved when Quarkus uses GraalVM 24 (currently 23)
#[cfg(feature = "test_quarkus_native")]
#[test]
pub fn verify_native_graalvm_quarkus_native() {
    build_quarkus_native_linux_executable().expect("Failed to build native executable");
    let digest = get_docker_image_digest();
    let port = next_open_port();

    run_quarkus_container(&digest, port);
    wait_until_endpoint_responds(port);
    get_true_or_false_value_from_http_endpoint(port);
    stop_quarkus_container(&digest);
}

///
/// A JBoss runner build
///
/// Builds a runnable Docker container, starts it, and compares output from the endpoint (that
/// loads the uniffi-java-bindgen generated library).
#[test]
pub fn verify_quarkus() {
    build_quarkus_linux_executable().expect("Failed to build native executable");
    let digest = get_docker_image_digest();
    let port = next_open_port();

    run_quarkus_container(&digest, port);
    wait_until_endpoint_responds(port);
    get_true_or_false_value_from_http_endpoint(port);
    stop_quarkus_container(&digest);
}

/// Runs the Quarkus runnable container on Docker
/// The container's name is the image's SHA256 digest. Later used to stop the container
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

/// 
/// Does HTTP calls to the Docker container, and stops when it gets output 
fn wait_until_endpoint_responds(port: u16) {
    let url = format!("http://127.0.0.1:{}/?trueOrNot=true", port);
    println!("URL: {url}");
    let client = reqwest::blocking::Client::new();
    for i in 1..10 {
        let v = client.get(&url).send();
        match v {
            Ok(r) => {
                if r.status().is_success() {
                    return;
                }
                let b = r.text().unwrap_or_default();
                println!("Error response {b}");
            }
            Err(e) => {
                println!("Error response {e:?}");
            }
        }
        println!("Try {i} of 10 failed. Waiting 1s");
        std::thread::sleep(std::time::Duration::from_millis(1000));
    }
}

///
/// Tests the output on the Quarkus endpoint
fn get_true_or_false_value_from_http_endpoint(port: u16) {
    let url = format!("http://127.0.0.1:{}/?trueOrNot=true", port);

    let client = reqwest::blocking::Client::new();
    let response = client.get(&url).send().expect("Failed to send GET request");
    assert!(response.status().is_success());
    let body = response.text().expect("Failed to get response body");
    assert_eq!(body, "true");

    let url = format!("http://127.0.0.1:{}/?trueOrNot=false", port);
    let response = client.get(&url).send().expect("Failed to send GET request");
    assert!(response.status().is_success());
    let body = response.text().expect("Failed to get response body");
    assert_eq!(body, "false");
}

///
/// Stops the Quarkus container, using its name (which was the image's SHA256 digest)
fn stop_quarkus_container(digest: &str) {
    // Stop the running quarkus container. The image digest was used as its container name
    let _child = Command::new("docker")
        .args(&["stop", digest])
        .spawn()
        .expect("Failed to stop quarkus container");
}

/// 
/// Get the next available port
fn next_open_port() -> u16 {
    // Try to bind to port 0 to let the OS assign an available port
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to an available port");
    // Get the port number
    let port = listener.local_addr().expect("Failed to get local address").port();
    // The listener is dropped here, which frees the port
    port
}

///
/// Quarkus prints the SHA256 it created on the target build folder
/// This is to parse it
fn get_docker_image_digest() -> String {
    // File is in tests/quarkus/scripts/service/target/jib-image.id
    let path = "tests/quarkus/scripts/service/target/jib-image.id";
    let contents = std::fs::read_to_string(path)
        .expect("Failed to read jib-image.id file");
    let digest = contents.trim();
    eprintln!("Docker image digest: {}", digest);

    // It is in format sha256:<digest>, get the <digest> part
    let digest = digest.split(':').nth(1).expect("Invalid digest format");
    digest.to_string()
}

///
/// Build the example test Quarkus service natively
fn build_quarkus_native_linux_executable() -> anyhow::Result<()> {
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
        .current_dir("tests/quarkus/scripts")
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
        .current_dir("tests/quarkus/scripts")
        .args(&[
            "package",
            "-Pnative",
            "-Dquarkus.native.resources.includes=com/sun/jna/linux-x86-64/libjnidispatch.so,com/sun/jna/linux-aarch64/libjnidispatch.so,libsay_true.so",
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

///
/// Build the JBoss runner Quarkus service
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
        .current_dir("tests/quarkus/scripts")
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
        .current_dir("tests/quarkus/scripts")
        .args(&[
            "package",
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

///
/// Get whether it is being run on ARM64 or AMD64
/// Used in the JIB image type
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

///
/// Run a command line child process and wait for it to finish
fn run_child_process_output_and_wait(mut child: Child) -> anyhow::Result<()> {
    let (stdout_handle, stderr_handle) = print_child_stdout_and_stderr(&mut child)?;
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

///
/// Print out a child process' stdout and stderr
fn print_child_stdout_and_stderr(child: &mut Child) -> anyhow::Result<(JoinHandle<()>, JoinHandle<()>)> {

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