use futures::stream::TryStreamExt;
use http::Uri;
use std::fs::File;
use std::io::prelude::*;
use warp::Buf;
use warp::Filter;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Upload {
    alt: String,
    #[serde(serialize_with = "optional_mime_serializer")]
    #[serde(deserialize_with = "optional_mime_deserializer")]
    content_type: Option<mime::Mime>,
}

fn optional_mime_serializer<S>(t: &Option<mime::Mime>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    t.clone().map(|v| v.to_string()).serialize(serializer)
}

fn optional_mime_deserializer<'de, D>(deserializer: D) -> Result<Option<mime::Mime>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct OptionalMimeVisitor;
    impl<'de> serde::de::Visitor<'de> for OptionalMimeVisitor {
        type Value = Option<mime::Mime>;

        fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
            formatter.write_str("an optional mime type")
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_string(OptionalMimeVisitor)
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Some(v.parse::<mime::Mime>())
                .transpose()
                .map_err(serde::de::Error::custom)
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }
    }

    deserializer.deserialize_option(OptionalMimeVisitor)
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
    content_type: Option<mime::Mime>,
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
        content_type: file_content_type
            .map(|ct| ct.parse::<mime::Mime>())
            .transpose()?,
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
            let ct = u.content_type.unwrap_or(mime::IMAGE_PNG);
            if ct.type_() == mime::VIDEO {
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
        }
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

fn to_rejection<E>(_e: E) -> warp::Rejection {
    warp::reject::reject()
}

async fn show_raw(id: String) -> Result<impl warp::Reply, warp::Rejection> {
    let upload: Upload = File::open(format!("public/{}.json", id))
        .map_err(to_rejection)
        .and_then(|file| serde_json::from_reader(file).map_err(to_rejection))?;

    let ct = upload.content_type.unwrap_or(mime::IMAGE_PNG);

    tokio::fs::File::open(format!("public/{}", id))
        .await
        .map(|file| tokio_util::codec::FramedRead::new(file, tokio_util::codec::BytesCodec::new()))
        .map(hyper::Body::wrap_stream)
        .map(|body| {
            http::Response::builder()
                .header("Content-Type", ct.to_string())
                .body(body)
        })
        .map_err(to_rejection)
}

#[tokio::main]
async fn main() {
    let index = warp::path::end().and(warp::get()).map(index);
    let upload = warp::path("upload")
        .and(warp::multipart::form().max_length(100_000_000))
        .and_then(upload);
    let show_html = warp::path!("file" / String).and(warp::get()).map(show_html);
    let show_raw = warp::path!("file" / "raw" / String).and_then(show_raw);

    let router = index.or(upload).or(show_html).or(show_raw);

    warp::serve(router).run(([127, 0, 0, 1], 3030)).await;
}
