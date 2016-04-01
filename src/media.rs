use rustc_serialize::{Decodable, Decoder};
use time::{Duration, Timespec, get_time};


#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Media {
    key: String,
    artist: String,
    title: String,
    length: Duration,
    uploaded_by: String,
}

impl Decodable for Media {
    fn decode<D: Decoder>(d: &mut D) -> Result<Self, D::Error> {
        d.read_map(|d, len| {
            let mut media_key = Err(d.error("no media key"));
            let mut artist = Err(d.error("no media artist field"));
            let mut title = Err(d.error("no media title field"));
            let mut length = Err(d.error("no media length field"));
            let mut uploaded_by = Err(d.error("no media uploadedByKey field"));
            for idx in 0..len {
                let key = try!(d.read_map_elt_key(idx, |d| d.read_str()));
                try!(d.read_map_elt_val(idx, |d| {
                    match &key[..] {
                        "key" => media_key = Decodable::decode(d),
                        "artist" => artist = Decodable::decode(d),
                        "title" => title = Decodable::decode(d),
                        "length" => length = decode_duration(d),
                        "uploadedByKey" => uploaded_by = d.read_str(),
                        _ => {} // ignore
                    }
                    Ok(())
                }))
            }
            Ok(Media {
                key: try!(media_key),
                artist: try!(artist),
                title: try!(title),
                length: try!(length),
                uploaded_by: try!(uploaded_by),
            })
        })
    }
}


#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Playing {
    requested_by: Option<String>,
    end_time: Timespec,
    media: Media
}

impl Decodable for Playing {
    fn decode<D: Decoder>(d: &mut D) -> Result<Self, D::Error> {
        d.read_map(|d, len| {
            let mut end_time = Err(d.error("no endTime field"));
            let mut media = Err(d.error("no media object"));
            let mut requested_by = Err(d.error("no byKey field "));
            let mut server_time = Err(d.error("no serverTime field"));
            for idx in 0..len {
                let key = try!(d.read_map_elt_key(idx, |d| d.read_str()));
                try!(d.read_map_elt_val(idx, |d| {
                    match &key[..] {
                        "byKey" => requested_by = Decodable::decode(d),
                        "endTime" => end_time = decode_timespec(d),
                        "media" => media = Decodable::decode(d),
                        "serverTime" => server_time = decode_timespec(d),
                        _ => {} // ignore
                    }
                    Ok(())
                }))
            }
            let end_time = end_time.map(|x| x + (get_time() - server_time.unwrap_or(get_time())));
            Ok(Playing {
                end_time: try!(end_time),
                media: try!(media),
                requested_by: try!(requested_by),
            })
        })
    }
}


#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    by: Option<String>,
    key: i64,
    media: Media,
}

impl Decodable for Request {
    fn decode<D: Decoder>(d: &mut D) -> Result<Self, D::Error> {
        d.read_map(|d, len| {
            let mut requested_by = Err(d.error("no byKey field"));
            let mut requested_key = Err(d.error("no key field"));
            let mut media = Err(d.error("no media object "));
            for idx in 0..len {
                let key = try!(d.read_map_elt_key(idx, |d| d.read_str()));
                try!(d.read_map_elt_val(idx, |d| {
                    match &key[..] {
                        "byKey" => requested_by = Decodable::decode(d),
                        "key" => requested_key = Decodable::decode(d),
                        "media" => media = Decodable::decode(d),
                        _ => {} // ignore
                    }
                    Ok(())
                }))
            }
            Ok(Request {
                by: try!(requested_by),
                key: try!(requested_key),
                media: try!(media),
            })
        })
    }
}


fn decode_timespec<D: Decoder>(d: &mut D) -> Result<Timespec, D::Error> {
    Decodable::decode(d)
        .map(|x: f64| {
            if x.is_nan() {
                // got an invalid time value from the server, just return 0?
                Timespec::new(0, 0)
            } else {
                Timespec::new(x.floor() as i64,
                              ((x%1_f64) * 10_f64.powi(6)).floor() as i32)
            }
        })
}

fn decode_duration<D: Decoder>(d: &mut D) -> Result<Duration, D::Error> {
    Decodable::decode(d)
        .map(|x: f64| {
            if x.is_nan() {
                // got an invalid time value from the server, just return 0?
                Duration::zero()
            } else {
                Duration::nanoseconds((x*10_f64.powi(9)) as i64)
            }
        })
}


#[cfg(test)]
mod tests {
    use rustc_serialize::json::decode as json_decode;
    use time::{Duration, Timespec};
    use super::*;

    fn expected_media() -> Media {
        Media {
            artist: String::from("Queens Of The Stone Age"),
            key: String::from("56bafc2c8dc01b4ea67fad9c"),
            length: Duration::seconds(231),
            title: String::from("In the Fade"),
            uploaded_by: String::from("dsprenkels"),
        }
    }

    fn expected_playing() -> Playing {
        Playing {
            end_time: Timespec::new(1459420207, 0),
            requested_by: Some(String::from("bkoks")),
            media: expected_media(),
        }
    }

    fn expected_request() -> Request {
        Request {
            by: Some(String::from("bkoks")),
            key: 3,
            media: expected_media(),
        }
    }

    #[test]
    fn decode_media() {
        let input = r#"
            {
               "artist":"Queens Of The Stone Age",
               "key":"56bafc2c8dc01b4ea67fad9c",
               "length":231,
               "title":"In the Fade",
               "uploadedByKey":"dsprenkels"
            }
        "#;
        assert_eq!(json_decode::<Media>(input).unwrap(), expected_media());
    }

    #[test]
    fn decode_playing() {
        let input = r#"
            {
              "byKey":"bkoks",
              "endTime":1459420207.0,
              "media":{
                "artist":"Queens Of The Stone Age",
                "key":"56bafc2c8dc01b4ea67fad9c",
                "length":231,
                "title":"In the Fade",
                "uploadedByKey":"dsprenkels"
              },
              "serverTime":1459419970.4571419
            }
        "#;
        let expected = expected_playing();
        let got = json_decode::<Playing>(input).unwrap();
        assert_eq!(got.requested_by, expected.requested_by);
        assert_eq!(got.media, expected.media);
    }

    #[test]
    fn decode_request() {
        let input = r#"
            {
              "byKey":"bkoks",
              "key":3,
              "media":{
                "artist":"Queens Of The Stone Age",
                "key":"56bafc2c8dc01b4ea67fad9c",
                "length":231,
                "title":"In the Fade",
                "uploadedByKey":"dsprenkels"
              }
            }
        "#;
        let expected = expected_request();
        let got = json_decode::<Request>(input).unwrap();
        assert_eq!(got, expected);
    }
}
