use rustc_serialize::{Decodable, Decoder};
use time::Duration;


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
            let mut media_key = None;
            let mut artist = None;
            let mut title = None;
            let mut length = None;
            let mut uploaded_by = None;
            for idx in 0..len {
                let key = try!(d.read_map_elt_key(idx, |d| d.read_str()));
                try!(d.read_map_elt_val(idx, |d| {
                    match &key[..] {
                        "key" => media_key = Some(try!(d.read_str())),
                        "artist" => artist = Some(try!(d.read_str())),
                        "title" => title = Some(try!(d.read_str())),
                        "length" => length = Some(try!(d.read_i64()))
                            .map(|x| Duration::seconds(x)),
                        "uploadedByKey" => uploaded_by = Some(try!(d.read_str())),
                        _ => {} // ignore
                    }
                    Ok(())
                }))
            }
            Ok(Media {
                key: try!(media_key.ok_or(d.error("no media key"))),
                artist: try!(artist.ok_or(d.error("no media artist field"))),
                title: try!(title.ok_or(d.error("no media title field"))),
                length: try!(length.ok_or(d.error("no media length field"))),
                uploaded_by: try!(uploaded_by.ok_or(d.error("no media uploadedByKey field"))),
            })
        })
    }
}


#[cfg(test)]
mod tests {
    use rustc_serialize::json::decode as json_decode;
    use time::Duration;
    use super::Media;

    #[test]
    fn decode() {
        let input = r#"
            {
               "artist":"Queens Of The Stone Age",
               "key":"56bafc2c8dc01b4ea67fad9c",
               "length":231,
               "title":"In the Fade",
               "uploadedByKey":"dsprenkels"
            }
        "#;
        let expected = Media {
            artist: String::from("Queens Of The Stone Age"),
            key: String::from("56bafc2c8dc01b4ea67fad9c"),
            length: Duration::seconds(231),
            title: String::from("In the Fade"),
            uploaded_by: String::from("dsprenkels"),
        };
        assert_eq!(json_decode::<Media>(input).unwrap(), expected);
    }

}
