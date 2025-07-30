use anyhow::Result;
use futures::SinkExt;
use futures::StreamExt;
use std::thread;
use std::time::Duration;
use tokio_tungstenite::connect_async;
use tracing::info;
mod common;

#[tokio::test]
async fn start_and_stop() -> Result<()> {
    // this will be the common pattern for acquiring a new relay:
    // start a fresh relay, on a port to-be-provided back to us:
    let relay = common::start_relay()?;
    // wait for the relay's webserver to start up and deliver a page:
    common::wait_for_healthy_relay(&relay).await?;
    let port = relay.port;
    // just make sure we can startup and shut down.
    // if we send a shutdown message before the server is listening,
    // we will get a SendError.  Keep sending until someone is
    // listening.
    loop {
        let shutdown_res = relay.shutdown_tx.send(());
        match shutdown_res {
            Ok(()) => {
                break;
            }
            Err(_) => {
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    // wait for relay to shutdown
    let thread_join = relay.handle.join();
    assert!(thread_join.is_ok());
    // assert that port is now available.
    assert!(common::port_is_available(port));
    Ok(())
}

#[tokio::test]
async fn relay_home_page() -> Result<()> {
    // get a relay and wait for startup...
    let relay = common::start_relay()?;
    common::wait_for_healthy_relay(&relay).await?;
    // tell relay to shutdown
    let _res = relay.shutdown_tx.send(());
    Ok(())
}

//#[tokio::test]
// Still inwork
async fn publish_test() -> Result<()> {
    // get a relay and wait for startup
    let relay = common::start_relay()?;
    common::wait_for_healthy_relay(&relay).await?;
    // open a non-secure websocket connection.
    let (mut ws, _res) = connect_async(format!("ws://localhost:{}", relay.port)).await?;
    // send a simple pre-made message
    let simple_event = r#"["EVENT", {"content": "hello world","created_at": 1691239763,
      "id":"f3ce6798d70e358213ebbeba4886bbdfacf1ecfd4f65ee5323ef5f404de32b86",
      "kind": 1,
      "pubkey": "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
      "sig": "30ca29e8581eeee75bf838171dec818af5e6de2b74f5337de940f5cc91186534c0b20d6cf7ad1043a2c51dbd60b979447720a471d346322103c83f6cb66e4e98",
      "tags": []}]"#;
    ws.send(simple_event.into()).await?;
    // get response from server, confirm it is an array with first element "OK"
    let event_confirm = ws.next().await;
    ws.close(None).await?;
    info!("event confirmed: {:?}", event_confirm);
    // open a new connection, and wait for some time to get the event.
    let (mut sub_ws, _res) = connect_async(format!("ws://localhost:{}", relay.port)).await?;
    let event_sub = r#"["REQ", "simple", {}]"#;
    sub_ws.send(event_sub.into()).await?;
    // read from subscription
    let _ws_next = sub_ws.next().await;
    let _res = relay.shutdown_tx.send(());
    Ok(())
}

#[tokio::test]
async fn search_nip50_test() -> anyhow::Result<()> {
    use serde_json::json;
    use futures::SinkExt;
    use futures::StreamExt;
    use tokio_tungstenite::connect_async;
    use std::time::Duration;
    use tokio::time::timeout;
    use nostr::{EventBuilder, Keys, Kind};
    use tokio::time::Instant;

    // Start relay
    let relay = common::start_relay()?;
    common::wait_for_healthy_relay(&relay).await?;
    let port = relay.port;

    // Generate a valid event using nostr crate
    let unique_content = "nip50searchtest-unique-12345";
    let keys = Keys::generate();
    let unsigned = EventBuilder::new(Kind::TextNote, unique_content);
    let event = unsigned.sign(&keys).await?;
    let event_json = json!(["EVENT", event]);

    // Open websocket and publish the event
    let (mut ws, _) = connect_async(format!("ws://localhost:{}", port)).await?;
    ws.send(event_json.to_string().into()).await?;
    // Wait for OK response
    let publish_response = ws.next().await;
    println!("Publish response: {:?}", publish_response);
    ws.close(None).await?;

    // Wait a bit to ensure the event is indexed
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Open a new websocket connection for search
    let (mut search_ws, _) = connect_async(format!("ws://localhost:{}", port)).await?;
    // NIP-50 search REQ
    let search_req = json!([
        "REQ",
        "search1",
        { "search": unique_content, "kinds": [1] }
    ]);
    search_ws.send(search_req.to_string().into()).await?;

    // Wait for the event response (with timeout)
    let start = Instant::now();
    let mut found = false;
    let mut all_responses = Vec::new();
    while start.elapsed() < Duration::from_secs(2) {
        if let Ok(Some(Ok(msg))) = timeout(Duration::from_millis(200), search_ws.next()).await {
            let text = msg.into_text().unwrap_or_default();
            println!("Search response: {}", text);
            all_responses.push(text.clone());
            if text.contains(unique_content) {
                found = true;
                break;
            }
        } else {
            break;
        }
    }
    assert!(found, "Search did not return the expected event. All responses: {:?}", all_responses);

    // Clean up
    let _ = relay.shutdown_tx.send(());
    Ok(())
}
