use actix::prelude::*;
use actix_web::HttpResponse;
use actix_web::{web, App, HttpServer};
use anyhow::Result;
use log::*;
use serde::Serialize;

#[derive(Serialize)]
enum FrameResult<'a> {
    Empty,
    Result(&'a pms5003::pms5003::Frame),
}

async fn last_frame(
    // monitor_addr: web::Data<Mutex<Addr<pms5003::pms5003::Monitor>>>,
    monitor_addr: web::Data<Addr<pms5003::pms5003::Monitor>>,
) -> HttpResponse {
    match monitor_addr.send(pms5003::pms5003::GetLastFrame).await {
        Ok(maybe_frame) => HttpResponse::Ok().body(match maybe_frame {
            Some(frame) => {
                let resp = serde_json::to_string_pretty(&FrameResult::Result(&frame)).unwrap();
                resp
            }
            None => serde_json::to_string_pretty(&FrameResult::Empty).unwrap(),
        }),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

fn main() -> Result<()> {
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();

    let mut rt = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .build()?;

    let rt_local = tokio::task::LocalSet::new();
    let sys = actix_rt::System::run_in_tokio(env!("CARGO_PKG_NAME"), &rt_local);

    rt_local.spawn_local(sys);
    rt_local.spawn_local(async {
        let monitor_addr = pms5003::pms5003::Monitor::new().start();

        info!("4");
        HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(monitor_addr.clone()))
                .service(web::resource("/last_frame").to(last_frame))
        })
        .disable_signals()
        .bind("0.0.0.0:8080")
        .unwrap()
        .run()
        .await
        .unwrap()
    });

    //Ok(sys.await?)
    Ok(rt.block_on(rt_local))
}
