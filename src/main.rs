use actix_files::NamedFile;
use actix_multipart::{Multipart, MultipartError};
use actix_web::{error::ErrorBadRequest, web, App, HttpResponse, HttpServer, Result};
use futures::stream::TryStreamExt;
use serde::{Deserialize, Serialize};
use std::io::prelude::*;

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

async fn index() -> HttpResponse {
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

    HttpResponse::Ok().body(body)
}

struct UploadForm {
    alt: String,
    file: Vec<u8>,
    content_type: Option<mime::Mime>,
}

async fn readall(
    stream: impl futures::Stream<Item = std::result::Result<actix_web::web::Bytes, MultipartError>>,
) -> std::result::Result<Vec<u8>, MultipartError> {
    stream
        .try_fold(vec![], |mut result, buf| {
            result.append(&mut buf.into_iter().collect());
            async move { Ok(result) }
        })
        .await
}

async fn parse_multipart_crap(
    mut form: Multipart,
) -> Result<UploadForm, Box<dyn std::error::Error>> {
    let mut alt: Option<String> = None;
    let mut file: Option<Vec<u8>> = None;
    let mut content_type: Option<mime::Mime> = None;

    while let Ok(Some(field)) = form.try_next().await {
        match field.content_disposition().unwrap().get_name() {
            Some("alt") => {
                alt = Some(String::from_utf8(readall(field).await?)?);
            }
            Some("file") => {
                content_type = Some(field.content_type().clone());
                file = Some(readall(field).await?);
            }
            _ => {}
        }
    }

    if alt.is_some() && file.is_some() {
        Ok(UploadForm {
            alt: alt.unwrap(),
            file: file.unwrap(),
            content_type: content_type,
        })
    } else {
        Err("".into())
    }
}

async fn upload(form: Multipart) -> Result<HttpResponse> {
    let upload_form = parse_multipart_crap(form)
        .await
        .map_err(|_e| ErrorBadRequest("dunno"))?;

    let id: u128 = rand::random();
    let mut file = std::fs::File::create(format!("public/{}", id)).unwrap();

    let metadata = Upload {
        alt: upload_form.alt,
        content_type: upload_form.content_type,
    };

    let metadata_file = std::fs::File::create(format!("public/{}.json", id)).unwrap();
    serde_json::to_writer_pretty(metadata_file, &metadata).unwrap();

    // TODO this is not async, maybe it should be
    file.write_all(&upload_form.file[..]).unwrap();

    Ok(HttpResponse::Found()
        .header("Location", format!("/file/{}", id))
        .body(""))
}

fn show_html(path: web::Path<(String,)>) -> HttpResponse {
    let id = path.into_inner().0;

    let upload: Result<Upload, _> =
        serde_json::from_reader(std::fs::File::open(format!("public/{}.json", id)).unwrap());

    match upload {
        Ok(u) => {
            let ct = u.content_type.unwrap_or(mime::IMAGE_PNG);
            if ct.type_() == mime::VIDEO {
                HttpResponse::Ok().body(format!(
                    "
                        {}
                        <h1><a href='/'>imghost</a></h1>
                        <video autoplay controls><source src='/file/raw/{}' /></video>
                    ",
                    CSS, id,
                ))
            } else {
                HttpResponse::Ok().body(format!(
                    "
                        {}
                        <h1><a href='/'>imghost</a></h1>
                        <img src='/file/raw/{}' />
                    ",
                    CSS, id,
                ))
            }
        }
        Err(_) => HttpResponse::NotFound().body(format!(
            "
                {}
                <h1><a href='/'>imghost</a></h1>
                <p>Not found.</p>
                ",
            CSS,
        )),
    }
}

fn to_bad_request<E>(_e: E) -> actix_web::error::Error {
    ErrorBadRequest("generic error")
}

async fn show_raw(path: web::Path<(String,)>) -> Result<NamedFile> {
    let id = path.into_inner().0;

    let upload: Upload = std::fs::File::open(format!("public/{}.json", id))
        .map_err(to_bad_request)
        .and_then(|file| serde_json::from_reader(file).map_err(to_bad_request))?;

    let ct = upload.content_type.unwrap_or(mime::IMAGE_PNG);

    let path = format!("public/{}", id);
    NamedFile::open(path)
        .map(|nf| nf.set_content_type(ct))
        .map_err(|_e| ErrorBadRequest("couldn't serve file"))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .route("/", web::get().to(index))
            .route("/upload", web::post().to(upload)) // max length 100_000_000
            .route("file/{id}", web::get().to(show_html))
            .route("file/raw/{id}", web::get().to(show_raw))
    })
    .bind("127.0.0.1:3030")?
    .run()
    .await
}
