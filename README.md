# imghost - self hosted image.. hosting?

A (work in progress) self hosted web thing that stores images.

This is how it looks like:

![Screenshot of the index page, displaying the imghost title and a form with an input field](home.png)

![Screenshot of the uploaded image page, displaying the imghost title and a the uploaded image](show.png)


## Routes

- `GET /` displays the upload form
- `POST /upload` receives an image and redirects to the view page
- `GET /image/<id>` displays an html page with an `img`
- `GET /image/raw/<id>` serves the raw image


## Things that would be nice to have

Alt text.

Javascript progressive enhancement to allow drag and drop. This means using
`fetch` to upload the file instead of submitting the form.

Mime type detection, instead of assuming it's an image. Right now I'm taking
whatever is uploaded and slapping it into an `<img/>`, but there's no
restriction on what you can upload.

Set the right `content-type` in the raw endpoint. I'm not sure if this should
be stored somewhere or if I should just detect it on each GET request.

Metadata stripping.

Video support. This would require mime type detection.

Password protection for uploads. I don't want to end up serving malware. The
password could be stored in a cookie or something so that you don't have to
type it every time.


## Why

It was an excuse to play with web rust. The code is full of `unwrap`s and it's
not meant to be deployed. It may be exploitable. I'm using `warp`, and handling
uploaded files looks a bit weird.
