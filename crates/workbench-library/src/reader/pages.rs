use crate::{archive, error, hash, Result};
use std::{cmp::Ordering, collections::BTreeMap, fs::File};
use workbench_storage::library_relative_path_is_valid;

pub(super) struct Chapter {
    pub id: String,
    pub title: String,
    pub pages: Vec<String>,
}

pub(super) fn index(file: &mut File) -> Result<Vec<Chapter>> {
    let mut archive = archive::open(file)?;
    let mut images = Vec::new();
    let mut managed = false;
    for index in 0..archive.len() {
        let entry = archive
            .by_index_raw(index)
            .map_err(|_| error("LIBRARY_ARCHIVE_INVALID"))?;
        let name = entry.name();
        if !library_relative_path_is_valid(name.trim_end_matches('/')) || entry.is_symlink() {
            return Err(error("LIBRARY_ARCHIVE_UNSAFE"));
        }
        if name == "_mangamonitor-layout.json" {
            managed = true;
        }
        if !entry.is_dir()
            && archive::is_image(name)
            && !name
                .split('/')
                .any(|part| part == "__MACOSX" || part.starts_with("._"))
        {
            images.push(name.to_owned());
        }
    }
    let chapter_images = images.iter().any(|name| name.contains('/'));
    // Managed cover.jpg is a generated thumbnail of page 1, not another page.
    // Unknown legacy covers remain readable rather than guessing duplication.
    if managed && chapter_images {
        images.retain(|name| name != "cover.jpg");
    }
    if images.is_empty() {
        return Err(error("READER_NO_PAGES"));
    }
    if images.len() > 50_000 {
        return Err(error("READER_PAGE_LIMIT"));
    }
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in images {
        let parent = name
            .rsplit_once('/')
            .map_or("", |(parent, _)| parent)
            .to_owned();
        groups.entry(parent).or_default().push(name);
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    if groups.len() > 5000 {
        return Err(error("READER_CHAPTER_LIMIT"));
    }
    groups.sort_by(|a, b| natural_cmp(&a.0, &b.0));
    Ok(groups
        .into_iter()
        .map(|(parent, mut pages)| {
            pages.sort_by(|a, b| natural_cmp(a, b));
            Chapter {
                id: hash(format!("zip-chapter:{parent}").as_bytes()),
                title: if parent.is_empty() {
                    "正文".into()
                } else {
                    parent
                },
                pages,
            }
        })
        .collect())
}

/// Compare digit runs by magnitude without parsing bounded machine integers.
/// Equal magnitudes use their original spelling for stable order.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let aa = a.as_bytes();
    let bb = b.as_bytes();
    let (mut i, mut j) = (0, 0);
    while i < aa.len() && j < bb.len() {
        if aa[i].is_ascii_digit() && bb[j].is_ascii_digit() {
            let (start_i, start_j) = (i, j);
            while i < aa.len() && aa[i].is_ascii_digit() {
                i += 1;
            }
            while j < bb.len() && bb[j].is_ascii_digit() {
                j += 1;
            }
            let ar = &a[start_i..i];
            let br = &b[start_j..j];
            let av = ar.trim_start_matches('0');
            let bv = br.trim_start_matches('0');
            let order = av
                .len()
                .cmp(&bv.len())
                .then_with(|| av.cmp(bv))
                .then_with(|| ar.len().cmp(&br.len()));
            if order != Ordering::Equal {
                return order;
            }
        } else {
            let order = aa[i].to_ascii_lowercase().cmp(&bb[j].to_ascii_lowercase());
            if order != Ordering::Equal {
                return order;
            }
            i += 1;
            j += 1;
        }
    }
    aa.len().cmp(&bb.len()).then_with(|| a.cmp(b))
}
