use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use arrow::array::{
    Array, ArrayRef, Float64Array, Float64Builder, Int64Array, Int64Builder, StringArray,
    StringBuilder,
};
use arrow::datatypes::{DataType as ArrowDataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use calamine::{open_workbook_auto, Data, Reader};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::arrow_writer::ArrowWriter;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// 导入结果，rows 直接序列化为 JSON 供前端表格展示
#[derive(Serialize)]
pub struct ImportResult {
    pub key: String,
    pub source: String,
    pub sheet: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub row_count: usize,
}

/// 已导入文件元信息
#[derive(Serialize)]
pub struct ImportMeta {
    pub key: String,
    pub source: String,
    pub sheet: String,
}

enum ColType {
    Int,
    Float,
    Str,
}

/// 数据目录：exe 同目录下的 data/，打包成 exe 后即 exe 旁文件夹
fn data_dir() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("无法获取程序路径: {e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "无法获取程序目录".to_string())?
        .join("data");
    std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建数据目录: {e}"))?;
    Ok(dir)
}

/// 读取源文件 -> 类型推断 -> 转 parquet 落盘 -> 返回表格数据
type Manifest = HashMap<String, (String, String)>; // key -> (source, sheet)

fn manifest_path() -> Result<std::path::PathBuf, String> {
    Ok(data_dir()?.join("manifest.json"))
}

fn load_manifest() -> Result<Manifest, String> {
    let p = manifest_path()?;
    if !p.exists() {
        return Ok(HashMap::new());
    }
    let s = std::fs::read_to_string(&p).map_err(|e| format!("读取清单失败: {e}"))?;
    serde_json::from_str(&s).map_err(|e| format!("清单格式错误: {e}"))
}

fn save_manifest(m: &Manifest) -> Result<(), String> {
    let s = serde_json::to_string(m).map_err(|e| e.to_string())?;
    std::fs::write(manifest_path()?, s).map_err(|e| format!("保存清单失败: {e}"))
}

fn update_manifest(key: &str, source: &str, sheet: &str) -> Result<(), String> {
    let mut m = load_manifest()?;
    m.insert(key.to_string(), (source.to_string(), sheet.to_string()));
    save_manifest(&m)
}

fn remove_manifest(key: &str) -> Result<(), String> {
    let mut m = load_manifest()?;
    m.remove(key);
    save_manifest(&m)
}

/// 导入文件：Excel 每个 sheet 转换成一个 parquet，csv 转换成一个。
/// 返回所有导入结果，key 为 parquet 文件名。
#[tauri::command]
pub fn import_file(path: String) -> Result<Vec<ImportResult>, String> {
    let src = Path::new(&path);
    if !src.exists() {
        return Err(format!("文件不存在: {path}"));
    }
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let source_name = src
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("文件")
        .to_string();

    let tables: Vec<(String, Vec<Option<String>>, Vec<Vec<Option<String>>>)> = match ext.as_str() {
        "csv" => {
            let (headers, rows) = read_csv(src)?;
            vec![(String::new(), headers, rows)]
        }
        "xlsx" | "xls" => read_excel_all(src)?,
        _ => return Err(format!("不支持的格式: {ext}")),
    };

    let mut results = Vec::new();
    for (sheet, headers, rows) in tables {
        if rows.is_empty() {
            continue;
        }
        let key = if sheet.is_empty() {
            format!("{stem}.parquet")
        } else {
            format!("{stem}_{}.parquet", sanitize(&sheet))
        };
        results.push(build_import(&key, &source_name, &sheet, headers, rows)?);
        update_manifest(&key, &source_name, &sheet)?;
    }
    if results.is_empty() {
        return Err("文件没有可导入的数据".to_string());
    }
    Ok(results)
}

/// 公共转换：类型推断 -> RecordBatch -> 写 parquet，key 为落盘文件名
fn build_import(
    key: &str,
    source: &str,
    sheet: &str,
    headers: Vec<Option<String>>,
    rows: Vec<Vec<Option<String>>>,
) -> Result<ImportResult, String> {
    let col_count = headers
        .len()
        .max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
    let columns: Vec<String> = (0..col_count)
        .map(|i| {
            let h = headers
                .get(i)
                .and_then(|s| s.as_deref())
                .unwrap_or("")
                .trim();
            if h.is_empty() {
                format!("列{}", i + 1)
            } else {
                h.to_string()
            }
        })
        .collect();

    let col_types: Vec<ColType> = (0..col_count)
        .map(|i| infer_type(rows.iter().map(|r| r.get(i).unwrap_or(&None))))
        .collect();

    let arrays: Vec<ArrayRef> = (0..col_count)
        .map(|i| {
            let values: Vec<Option<String>> =
                rows.iter().map(|r| r.get(i).cloned().flatten()).collect();
            build_array(&values, &col_types[i])
        })
        .collect();

    let mut used: HashSet<String> = HashSet::new();
    let fields: Vec<Field> = columns
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let mut unique = name.clone();
            let mut n = 1;
            while used.contains(&unique) {
                unique = format!("{name}_{n}");
                n += 1;
            }
            used.insert(unique.clone());
            Field::new(unique, arrow_type(&col_types[i]), true)
        })
        .collect();
    let schema = Arc::new(Schema::new(fields));
    let batch = RecordBatch::try_new(schema, arrays).map_err(|e| format!("构建数据失败: {e}"))?;

    let out_path = data_dir()?.join(key);
    let file = File::create(&out_path).map_err(|e| format!("无法创建 parquet 文件: {e}"))?;
    let mut writer = ArrowWriter::try_new(file, batch.schema(), None)
        .map_err(|e| format!("创建 parquet 写入器失败: {e}"))?;
    writer.write(&batch).map_err(|e| format!("写入 parquet 失败: {e}"))?;
    writer.close().map_err(|e| format!("保存 parquet 失败: {e}"))?;

    Ok(ImportResult {
        key: key.to_string(),
        source: source.to_string(),
        sheet: sheet.to_string(),
        columns,
        rows: vec![],
        row_count: rows.len(),
    })
}

/// sheet 名写入文件名前的安全化：替换非法字符
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect()
}

fn read_csv(path: &Path) -> Result<(Vec<Option<String>>, Vec<Vec<Option<String>>>), String> {
    let mut reader = csv::Reader::from_path(path).map_err(|e| format!("读取 CSV 失败: {e}"))?;
    let headers: Vec<Option<String>> = reader
        .headers()
        .map_err(|e| format!("读取表头失败: {e}"))?
        .iter()
        .map(|h| Some(h.to_string()))
        .collect();
    let mut rows = Vec::new();
    for rec in reader.records() {
        let rec = rec.map_err(|e| format!("读取 CSV 行失败: {e}"))?;
        rows.push(
            rec.iter()
                .map(|v| {
                    let t = v.trim();
                    if t.is_empty() {
                        None
                    } else {
                        Some(t.to_string())
                    }
                })
                .collect(),
        );
    }
    Ok((headers, rows))
}

/// 读取 Excel 所有 sheet，返回 (sheet 名, 表头, 行数据)
fn read_excel_all(
    path: &Path,
) -> Result<Vec<(String, Vec<Option<String>>, Vec<Vec<Option<String>>>)>, String> {
    let mut wb = open_workbook_auto(path).map_err(|e| format!("无法打开 Excel 文件: {e}"))?;
    let names = wb.sheet_names().to_vec();
    if names.is_empty() {
        return Err("Excel 中没有工作表".to_string());
    }
    let mut out = Vec::new();
    for name in names {
        let range = wb
            .worksheet_range(&name)
            .map_err(|e| format!("读取工作表 {name} 失败: {e}"))?;
        let mut rows: Vec<Vec<Option<String>>> = range
            .rows()
            .map(|row| row.iter().map(cell_to_string).collect())
            .collect();
        if rows.is_empty() {
            continue;
        }
        let headers = rows.remove(0);
        out.push((name, headers, rows));
    }
    Ok(out)
}

fn cell_to_string(cell: &Data) -> Option<String> {
    match cell {
        Data::Empty => None,
        Data::String(s) => Some(s.clone()),
        Data::Float(f) => Some(f.to_string()),
        Data::Int(i) => Some(i.to_string()),
        Data::Bool(b) => Some(b.to_string()),
        Data::DateTime(dt) => Some(dt.to_string()),
        Data::DateTimeIso(s) => Some(s.clone()),
        Data::DurationIso(s) => Some(s.clone()),
        Data::Error(e) => Some(format!("错误: {e}")),
    }
}

/// 列类型推断：全部为整数 -> Int，全部可解析为数字 -> Float，否则 String
fn infer_type<'a>(values: impl Iterator<Item = &'a Option<String>>) -> ColType {
    let mut all_int = true;
    let mut all_num = true;
    for v in values {
        if let Some(s) = v {
            if s.parse::<i64>().is_err() {
                all_int = false;
            }
            if s.parse::<f64>().is_err() {
                all_num = false;
            }
        }
    }
    if !all_num {
        ColType::Str
    } else if all_int {
        ColType::Int
    } else {
        ColType::Float
    }
}

fn arrow_type(t: &ColType) -> ArrowDataType {
    match t {
        ColType::Int => ArrowDataType::Int64,
        ColType::Float => ArrowDataType::Float64,
        ColType::Str => ArrowDataType::Utf8,
    }
}

fn build_array(values: &[Option<String>], t: &ColType) -> ArrayRef {
    match t {
        ColType::Int => {
            let mut b = Int64Builder::new();
            for v in values {
                match v {
                    Some(s) => b.append_value(s.parse().unwrap_or(0)),
                    None => b.append_null(),
                }
            }
            Arc::new(b.finish())
        }
        ColType::Float => {
            let mut b = Float64Builder::new();
            for v in values {
                match v {
                    Some(s) => b.append_value(s.parse().unwrap_or(0.0)),
                    None => b.append_null(),
                }
            }
            Arc::new(b.finish())
        }
        ColType::Str => {
            let mut b = StringBuilder::new();
            for v in values {
                b.append_option(v.as_deref());
            }
            Arc::new(b.finish())
        }
    }
}

/// 列出 data/ 下已转换的 parquet 文件（含源文件名/sheet），按修改时间最新在前
#[tauri::command]
pub fn list_imported() -> Result<Vec<ImportMeta>, String> {
    let dir = data_dir()?;
    let manifest = load_manifest().unwrap_or_default();
    let mut items: Vec<(String, i64)> = std::fs::read_dir(&dir)
        .map_err(|e| format!("读取数据目录失败: {e}"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".parquet"))
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let modified = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            (name, modified)
        })
        .collect();
    items.sort_by(|a, b| b.1.cmp(&a.1)); // 最新在前
    Ok(items
        .into_iter()
        .map(|(key, _)| {
            let (source, sheet) = manifest.get(&key).cloned().unwrap_or_default();
            ImportMeta {
                key,
                source,
                sheet,
            }
        })
        .collect())
}

/// 单个筛选条件：field 为列索引（None 表示全部字段），value 为模糊关键字
#[derive(Deserialize)]
pub struct Filter {
    pub field: Option<usize>,
    pub value: Option<String>,
}

/// 分页读取请求（单个结构体参数，避免 Tauri 多参数传递问题）
#[derive(Deserialize)]
pub struct ReadRequest {
    pub key: String,
    pub offset: usize,
    pub limit: usize,
    pub filters: Vec<Filter>,
}

/// 分页读取 parquet，支持多条件模糊筛选（全部条件同时满足）。
/// offset/limit 控制分页，filters 为筛选条件列表。
#[tauri::command]
pub fn read_parquet(req: ReadRequest) -> Result<ImportResult, String> {
    let key = req.key;
    let offset = req.offset;
    let limit = req.limit.min(500);
    let filters = req.filters;
    let path = data_dir()?.join(&key);
    if !path.exists() {
        return Err(format!("文件不存在: {key}"));
    }
    let file = File::open(&path).map_err(|e| format!("无法打开文件: {e}"))?;
    let builder =
        ParquetRecordBatchReaderBuilder::try_new(file).map_err(|e| format!("读取失败: {e}"))?;
    let columns: Vec<String> = builder
        .schema()
        .fields()
        .iter()
        .map(|f| f.name().to_string())
        .collect();
    // parquet 元数据总行数：无筛选时直接用，避免扫全表
    let meta_rows = builder.metadata().file_metadata().num_rows() as usize;
    let reader = builder.build().map_err(|e| format!("解析失败: {e}"))?;

    let has_filter = filters
        .iter()
        .any(|f| f.value.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false));
    let start = offset;
    let end = offset + limit;
    let mut matched = 0usize;
    let mut page_rows: Vec<Vec<Value>> = Vec::new();

    let collect_row = |batch: &RecordBatch, i: usize, num_cols: usize, out: &mut Vec<Vec<Value>>| {
        let row = (0..num_cols)
            .map(|c| array_value(batch.column(c), i))
            .collect();
        out.push(row);
    };

    if has_filter {
        // 有筛选：必须全量遍历，所有条件 AND 同时满足才匹配
        for batch in reader {
            let batch = batch.map_err(|e| format!("读取数据失败: {e}"))?;
            let num_cols = batch.num_columns();
            for i in 0..batch.num_rows() {
                let pass = filters.iter().all(|f| {
                    let Some(raw) = f.value.as_deref() else {
                        return true;
                    };
                    let kw = raw.trim().to_lowercase();
                    if kw.is_empty() {
                        return true;
                    }
                    match f.field {
                        Some(fi) => row_matches(&batch, i, fi, &kw),
                        None => row_matches_any(&batch, i, &kw, num_cols),
                    }
                });
                if !pass {
                    continue;
                }
                if matched >= start && matched < end {
                    collect_row(&batch, i, num_cols, &mut page_rows);
                }
                matched += 1;
            }
        }
    } else {
        // 无筛选：只读窗口，总数用元数据
        'outer: for batch in reader {
            let batch = batch.map_err(|e| format!("读取数据失败: {e}"))?;
            let num_cols = batch.num_columns();
            for i in 0..batch.num_rows() {
                if matched >= start && matched < end {
                    collect_row(&batch, i, num_cols, &mut page_rows);
                }
                matched += 1;
                if matched >= end {
                    break 'outer;
                }
            }
        }
    }

    let row_count = if has_filter { matched } else { meta_rows };

    let (source, sheet) = load_manifest()
        .unwrap_or_default()
        .get(&key)
        .cloned()
        .unwrap_or_default();
    Ok(ImportResult {
        key,
        source,
        sheet,
        columns,
        rows: page_rows,
        row_count,
    })
}

/// 任一列命中即匹配（全局模糊搜索）
fn row_matches_any(batch: &RecordBatch, i: usize, fv: &str, num_cols: usize) -> bool {
    (0..num_cols).any(|c| row_matches(batch, i, c, fv))
}

/// arrow 层的字段值模糊匹配，避免为全表构建 JSON
fn row_matches(batch: &RecordBatch, i: usize, col: usize, fv: &str) -> bool {
    let arr = batch.column(col);
    match arr.data_type() {
        ArrowDataType::Utf8 => arr
            .as_any()
            .downcast_ref::<StringArray>()
            .map(|a| a.is_valid(i) && a.value(i).to_lowercase().contains(fv))
            .unwrap_or(false),
        ArrowDataType::Int64 => arr
            .as_any()
            .downcast_ref::<Int64Array>()
            .map(|a| a.is_valid(i) && a.value(i).to_string().contains(fv))
            .unwrap_or(false),
        ArrowDataType::Float64 => arr
            .as_any()
            .downcast_ref::<Float64Array>()
            .map(|a| a.is_valid(i) && a.value(i).to_string().contains(fv))
            .unwrap_or(false),
        _ => false,
    }
}

fn array_value(arr: &ArrayRef, i: usize) -> Value {
    let null = || Value::Null;
    match arr.data_type() {
        ArrowDataType::Int64 => arr
            .as_any()
            .downcast_ref::<Int64Array>()
            .map(|a| if a.is_null(i) { null() } else { json!(a.value(i)) })
            .unwrap_or_else(null),
        ArrowDataType::Float64 => arr
            .as_any()
            .downcast_ref::<Float64Array>()
            .map(|a| if a.is_null(i) { null() } else { json!(a.value(i)) })
            .unwrap_or_else(null),
        ArrowDataType::Utf8 => arr
            .as_any()
            .downcast_ref::<StringArray>()
            .map(|a| {
                if a.is_null(i) {
                    null()
                } else {
                    json!(a.value(i))
                }
            })
            .unwrap_or_else(null),
        _ => null(),
    }
}

/// 缓存条目
#[derive(Serialize)]
pub struct CacheEntry {
    pub name: String,
    pub kind: String, // "file" | "dir"
    pub size: u64,
    pub modified: i64,
}

#[derive(Serialize)]
pub struct CacheInfo {
    pub path: String,
    pub entries: Vec<CacheEntry>,
}

/// 列出 data/ 目录下的文件和文件夹
#[tauri::command]
pub fn list_cache() -> Result<CacheInfo, String> {
    let dir = data_dir()?;
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("读取目录失败: {e}"))? {
        let entry = entry.map_err(|e| format!("读取目录项失败: {e}"))?;
        let md = entry.metadata().map_err(|e| format!("读取元数据失败: {e}"))?;
        let modified = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        entries.push(CacheEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            kind: if md.is_dir() { "dir".to_string() } else { "file".to_string() },
            size: md.len(),
            modified,
        });
    }
    Ok(CacheInfo {
        path: dir.to_string_lossy().to_string(),
        entries,
    })
}

/// 删除 data/ 下指定文件或文件夹（含路径穿越防护），同步清理 manifest
#[tauri::command]
pub fn delete_cache(name: String) -> Result<(), String> {
    let dir = data_dir()?;
    let base = dir
        .canonicalize()
        .map_err(|e| format!("无法解析数据目录: {e}"))?;
    let canonical = dir
        .join(&name)
        .canonicalize()
        .map_err(|_| format!("目标不存在: {name}"))?;
    if !canonical.starts_with(&base) {
        return Err("非法路径".to_string());
    }
    if canonical.is_dir() {
        std::fs::remove_dir_all(&canonical).map_err(|e| format!("删除文件夹失败: {e}"))?;
    } else {
        std::fs::remove_file(&canonical).map_err(|e| format!("删除文件失败: {e}"))?;
        if name.ends_with(".parquet") {
            let _ = remove_manifest(&name);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::io::Write;

    /// 对真实导入的 parquet 验证筛选（文件存在才跑）
    #[test]
    fn filter_real_note_parquet() {
        let real = r"D:\project\rvt-file\src-tauri\target\debug\data\note_测试1.parquet";
        if !std::path::Path::new(real).exists() {
            eprintln!("真实文件不存在，跳过");
            return;
        }
        let dir = data_dir().unwrap();
        let key = "note_测试1.parquet";
        std::fs::copy(real, dir.join(key)).unwrap();

        let info = read_parquet(ReadRequest {
            key: key.to_string(),
            offset: 0,
            limit: 1,
            filters: vec![],
        })
        .unwrap();
        eprintln!("columns = {:?}", info.columns);
        let idx = info.columns.iter().position(|c| c == "资产性质");
        eprintln!("资产性质 index = {:?}", idx);
        if let Some(fi) = idx {
            let filtered = read_parquet(ReadRequest {
                key: key.to_string(),
                offset: 0,
                limit: 500,
                filters: vec![Filter {
                    field: Some(fi),
                    value: Some("3".to_string()),
                }],
            })
            .unwrap();
            eprintln!("筛选 资产性质=3 -> row_count = {}", filtered.row_count);
            if filtered.row_count > 0 && filtered.row_count < 10 {
                eprintln!("首行 = {:?}", filtered.rows[0]);
            }
        }
        let _ = std::fs::remove_file(dir.join(key));
    }

    /// 模拟 Tauri 把前端 JSON 参数反序列化到 read_parquet 的参数
    #[test]
    fn tauri_args_serde() {
        // 无筛选
        let a: ReadRequest =
            serde_json::from_str(r#"{"key":"x","offset":0,"limit":10,"filters":[]}"#).unwrap();
        assert!(a.filters.is_empty());
        // 全局搜索（未选字段）
        let b: ReadRequest = serde_json::from_str(
            r#"{"key":"x","offset":0,"limit":10,"filters":[{"field":null,"value":"3"}]}"#,
        )
        .unwrap();
        assert_eq!(b.filters.len(), 1);
        assert_eq!(b.filters[0].field, None);
        assert_eq!(b.filters[0].value, Some("3".to_string()));
        // 多条件 AND
        let c: ReadRequest = serde_json::from_str(
            r#"{"key":"x","offset":0,"limit":10,"filters":[{"field":7,"value":"3"},{"field":1,"value":"abc"}]}"#,
        )
        .unwrap();
        assert_eq!(c.filters.len(), 2);
        assert_eq!(c.filters[1].field, Some(1));
        assert_eq!(c.filters[1].value, Some("abc".to_string()));
    }

    #[test]
    fn import_csv_infers_types_and_writes_parquet() {
        let tmp = std::env::temp_dir().join("rvt_import_test.csv");
        let mut f = File::create(&tmp).unwrap();
        f.write_all(b"name,age,score\nAlice,25,90.5\nBob,30,\n")
            .unwrap();

        let results = import_file(tmp.to_string_lossy().to_string()).unwrap();
        assert_eq!(results.len(), 1);
        let res = &results[0];
        assert_eq!(res.columns, vec!["name", "age", "score"]);
        assert_eq!(res.row_count, 2);
        // import 不返回全量数据
        assert!(res.rows.is_empty());
        // parquet 已落盘
        assert!(data_dir().unwrap().join("rvt_import_test.parquet").exists());

        // 分页读回：表头与数据一致
        let back = read_parquet(ReadRequest {
            key: "rvt_import_test.parquet".to_string(),
            offset: 0,
            limit: 10,
            filters: vec![],
        })
        .unwrap();
        assert_eq!(back.columns, vec!["name", "age", "score"]);
        assert_eq!(back.row_count, 2);
        assert_eq!(back.rows[0], vec![json!("Alice"), json!(25), json!(90.5)]);
        assert_eq!(back.rows[1], vec![json!("Bob"), json!(30), Value::Null]);

        // 字段值模糊筛选：name 列含 "ali"
        let filtered = read_parquet(ReadRequest {
            key: "rvt_import_test.parquet".to_string(),
            offset: 0,
            limit: 10,
            filters: vec![Filter {
                field: Some(0),
                value: Some("ali".to_string()),
            }],
        })
        .unwrap();
        assert_eq!(filtered.row_count, 1);
        assert_eq!(filtered.rows[0][0], json!("Alice"));

        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(data_dir().unwrap().join("rvt_import_test.parquet"));
    }
}
