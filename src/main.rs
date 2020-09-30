use futures::stream::TryStreamExt;
use http::Uri;
use std::fs::File;
use std::io::prelude::*;
use warp::Buf;
use warp::Filter;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Upload {
    alt: String,
    // I would like this to be Option<mime::Mime>, but lol serde
    content_type: Option<String>,
}

const CSS: &str = "
<style>
  body { text-align: center; }
  h1 a { color: white; }
  h1 { display: inline-block; background-color: #AB2346; padding: 10px 20px; }
  video, img { display: block; max-width: 100%; margin: 0px auto; }
  input { display: block; margin: 10px auto; }
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
            <input type='text' name='alt' />
            <input type='submit' value='Upload' />
        </form>
    ",
        CSS,
    );

    warp::reply::html(body)
}

struct UploadForm {
    alt: String,
    file: Vec<u8>,
    content_type: Option<String>,
}

async fn readall(
    stream: impl futures::Stream<Item = Result<impl Buf, warp::Error>>,
) -> Result<Vec<u8>, warp::Error> {
    stream
        .try_fold(vec![], |mut result, buf| {
            result.append(&mut buf.bytes().into());
            async move { Ok(result) }
        })
        .await
}

async fn parse_multipart_crap(
    form: warp::multipart::FormData,
) -> Result<UploadForm, Box<dyn std::error::Error>> {
    let mut parts = form.try_collect::<Vec<_>>().await?;

    let mut get_part = |name: &str| {
        parts
            .iter()
            .position(|part| part.name() == name)
            .map(|p| parts.swap_remove(p))
            .ok_or(format!("{} part not found", name))
    };

    let alt = get_part("alt")?;
    let file = get_part("file")?;

    let file_content_type = file.content_type().map(|ct| ct.to_string());
    let file_contents = readall(file.stream()).await?;
    let alt_text = readall(alt.stream()).await?;

    Ok(UploadForm {
        alt: String::from_utf8(alt_text)?,
        file: file_contents,
        content_type: file_content_type,
    })
}

async fn upload(form: warp::multipart::FormData) -> Result<impl warp::Reply, warp::Rejection> {
    let upload_form = parse_multipart_crap(form)
        .await
        .map_err(|_e| warp::reject::reject())?;

    let id: u128 = rand::random();
    let mut file = File::create(format!("public/{}", id)).unwrap();

    let metadata = Upload {
        alt: upload_form.alt,
        content_type: upload_form.content_type,
    };
    let metadata_file = File::create(format!("public/{}.json", id)).unwrap();
    serde_json::to_writer_pretty(metadata_file, &metadata).unwrap();

    file.write_all(&upload_form.file[..]).unwrap();

    Ok(warp::redirect::redirect(
        format!("/file/{}", id).parse::<Uri>().unwrap(),
    ))
}

fn show_html(id: String) -> impl warp::Reply {
    let upload: Result<Upload, _> =
        serde_json::from_reader(File::open(format!("public/{}.json", id)).unwrap());

    match upload {
        Ok(u) => {
            let ct = u.content_type.unwrap_or("image/png".to_string());
            if ct.starts_with("video/") {
                warp::reply::html(format!(
                    "
                        {}
                        <h1><a href='/'>imghost</a></h1>
                        <video autoplay controls><source src='/file/raw/{}' /></video>
                    ",
                    CSS, id,
                ))
            } else {
                warp::reply::html(format!(
                    "
                        {}
                        <h1><a href='/'>imghost</a></h1>
                        <img src='/file/raw/{}' />
                    ",
                    CSS, id,
                ))
            }
        },
        Err(_) => warp::reply::html(format!(
            "
                {}
                <h1><a href='/'>imghost</a></h1>
                <p>Not found.</p>
                ",
            CSS,
        )),
    }
}

#[tokio::main]
async fn main() {
    let index = warp::path::end().and(warp::get()).map(index);
    let upload = warp::path("upload")
        .and(warp::multipart::form().max_length(100_000_000))
        .and_then(upload);
    let show_html = warp::path!("file" / String)
        .and(warp::get())
        .map(show_html);
    let show_raw = warp::path!("file" / "raw" / ..).and(warp::fs::dir("public"));

    let router = index.or(upload).or(show_html).or(show_raw);

    warp::serve(router).run(([127, 0, 0, 1], 3030)).await;
}
