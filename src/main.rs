use futures::stream::TryStreamExt;
use http::Uri;
use std::fs::File;
use std::io::prelude::*;
use warp::Buf;
use warp::Filter;

const CSS: &str = "
<style>
  body { text-align: center; }
  h1 a { color: white; }
  h1 { display: inline-block; background-color: #AB2346; padding: 10px 20px; }
  img { display: block; max-width: 100%; margin: 0px auto; }
  input { display: block; margin: 0px auto; }
  #dropzone { width: 600px; max-width: 90%; height: 300px; background-color: red; line-height: 300px; }
  #dropzone.hidden { display: none; }
  #dropzone.active { background-color: blue; }
</style>";

const JS: &str = "
<script type='text/javascript'>
    var droppedFiles = [];

    var form = document.getElementById('upload');
    var dropzone = document.getElementById('dropzone')

    form.addEventListener('submit', function(evt) {
        if (droppedFiles.length > 0) {
            // ajax it
            evt.preventDefault();
            return false;
        }
    });

    dropzone.addEventListener('dragover', function(evt) {
        evt.preventDefault();
    });

    dropzone.addEventListener('dragenter', function(evt) {
        dropzone.classList.add('active');
        evt.preventDefault();
    });

    dropzone.addEventListener('dragleave', function(evt) {
        dropzone.classList.remove('active');
        evt.preventDefault();
    });

    dropzone.classList.remove('hidden');
    dropzone.addEventListener('drop', function(evt) {
        evt.preventDefault();
        dropzone.classList.remove('active');
        droppedFiles = evt.dataTransfer.files;
    });
</script>";

fn index() -> impl warp::Reply {
    let body = format!(
        "
        {}
        <h1><a href='/'>imghost</a></h1>
        <form id='upload' method='post' action='upload' enctype='multipart/form-data'>
            <div id='dropzone' class='hidden'>
                Drop file here
            </div>

            <input type='file' name='file' />
            <input type='submit' value='Upload' />
        </form>
    ",
        CSS,
    );

    warp::reply::html(body)
}

async fn upload(form: warp::multipart::FormData) -> Result<impl warp::Reply, warp::Rejection> {
    let mut parts: Vec<warp::multipart::Part> = form
        .try_collect()
        .await
        .map_err(|_e| warp::reject::reject())?;

    let id: u128 = rand::random();
    let mut file = File::create(format!("public/{}", id)).unwrap();

    file.write_all(parts[0].data().await.unwrap().unwrap().bytes())
        .unwrap();

    Ok(warp::redirect::redirect(
        format!("/image/{}", id).parse::<Uri>().unwrap(),
    ))
}

fn show_html(id: String) -> impl warp::Reply {
    warp::reply::html(format!(
        "
        {}
        <h1><a href='/'>imghost</a></h1>
        <img src='/image/raw/{}' />
        ",
        CSS, id,
    ))
}

#[tokio::main]
async fn main() {
    let index = warp::path::end().and(warp::get()).map(index);
    let upload = warp::path("upload")
        .and(warp::multipart::form().max_length(100_000_000))
        .and_then(upload);
    let show_html = warp::path!("image" / String)
        .and(warp::get())
        .map(show_html);
    let show_raw = warp::path!("image" / "raw" / ..).and(warp::fs::dir("public"));

    let router = index.or(upload).or(show_html).or(show_raw);

    warp::serve(router).run(([127, 0, 0, 1], 3030)).await;
}
