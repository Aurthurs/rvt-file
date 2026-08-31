use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::Path;
use std::sync::{Arc, Mutex};

use arrow::array::{Array, ArrayRef, Float64Array, Int64Array, StringArray, StringBuilder};
use arrow::datatypes::{DataType as ArrowDataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use calamine::{open_workbook_auto, Data, Reader};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::arrow_writer::ArrowWriter;
use serde_json::Value;
use tauri::Manager;

/// 导入结果，rows 直接序列化为 JSON 供前端表格展示
#[derive(serde::Serialize)]
pub struct ImportResult {
    pub key: String,
    pub source: String,
    pub sheet: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub row_count: usize,
}

/// 已导入文件元信息
#[derive(serde::Serialize)]
pub struct ImportMeta {
    pub key: String,
    pub source: String,
    pub sheet: String,
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

/// 保护 manifest.json 的并发读写（多命令/多线程安全）
static MANIFEST_LOCK: Mutex<()> = Mutex::new(());

fn manifest_path() -> Result<std::path::PathBuf, String> {
    Ok(data_dir()?.join("manifest.json"))
}

fn load_manifest_inner() -> Result<Manifest, String> {
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

fn load_manifest() -> Result<Manifest, String> {
    let _g = MANIFEST_LOCK.lock().map_err(|_| "清单锁失效".to_string())?;
    load_manifest_inner()
}

fn update_manifest(key: &str, source: &str, sheet: &str) -> Result<(), String> {
    let _g = MANIFEST_LOCK.lock().map_err(|_| "清单锁失效".to_string())?;
    let mut m = load_manifest_inner()?;
    m.insert(key.to_string(), (source.to_string(), sheet.to_string()));
    save_manifest(&m)
}

fn remove_manifest(key: &str) -> Result<(), String> {
    let _g = MANIFEST_LOCK.lock().map_err(|_| "清单锁失效".to_string())?;
    let mut m = load_manifest_inner()?;
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

    // 全部按字符串存储，不做类型推断（避免 "06" 变 6、精度丢失等问题）
    let arrays: Vec<ArrayRef> = (0..col_count)
        .map(|i| {
            let values: Vec<Option<String>> =
                rows.iter().map(|r| r.get(i).cloned().flatten()).collect();
            let mut b = StringBuilder::new();
            for v in &values {
                b.append_option(v.as_deref());
            }
            Arc::new(b.finish()) as ArrayRef
        })
        .collect();

    let mut used: HashSet<String> = HashSet::new();
    let fields: Vec<Field> = columns
        .iter()
        .enumerate()
        .map(|(_, name)| {
            let mut unique = name.clone();
            let mut n = 1;
            while used.contains(&unique) {
                unique = format!("{name}_{n}");
                n += 1;
            }
            used.insert(unique.clone());
            Field::new(unique, ArrowDataType::Utf8, true)
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
#[derive(serde::Deserialize)]
pub struct Filter {
    pub field: Option<usize>,
    pub value: Option<String>,
}

/// 分页读取请求（单个结构体参数，避免 Tauri 多参数传递问题）
#[derive(serde::Deserialize)]
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
    // 统一按字符串返回，不依赖类型推断（避免数字丢失前导零/精度、科学计数法等）
    match arr.data_type() {
        ArrowDataType::Int64 => arr
            .as_any()
            .downcast_ref::<Int64Array>()
            .map(|a| {
                if a.is_null(i) {
                    null()
                } else {
                    Value::String(a.value(i).to_string())
                }
            })
            .unwrap_or_else(null),
        ArrowDataType::Float64 => arr
            .as_any()
            .downcast_ref::<Float64Array>()
            .map(|a| {
                if a.is_null(i) {
                    null()
                } else {
                    Value::String(a.value(i).to_string())
                }
            })
            .unwrap_or_else(null),
        ArrowDataType::Utf8 => arr
            .as_any()
            .downcast_ref::<StringArray>()
            .map(|a| {
                if a.is_null(i) {
                    null()
                } else {
                    Value::String(a.value(i).to_string())
                }
            })
            .unwrap_or_else(null),
        _ => null(),
    }
}

/// 缓存条目
#[derive(serde::Serialize)]
pub struct CacheEntry {
    pub name: String,
    pub kind: String, // "file" | "dir"
    pub size: u64,
    pub modified: i64,
}

#[derive(serde::Serialize)]
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
        let name = entry.file_name().to_string_lossy().to_string();
        // manifest.json 是内部清单，不展示
        if name == "manifest.json" {
            continue;
        }
        let md = entry.metadata().map_err(|e| format!("读取元数据失败: {e}"))?;
        let modified = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        entries.push(CacheEntry {
            name,
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
    if name == "manifest.json" {
        return Err("manifest.json 为系统清单文件，不允许删除".to_string());
    }
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

/// 导出请求
#[derive(serde::Deserialize)]
pub struct ExportRequest {
    pub keys: Vec<String>,
    pub format: String, // "csv" | "xlsx" | "parquet"
    pub output_dir: String,
    pub merge: bool, // true = 合并到一个 xlsx，每个文件一个 sheet
    pub file_name: Option<String>, // 合并导出时的自定义文件名
}

#[derive(serde::Serialize)]
pub struct ExportResult {
    pub exported: Vec<String>,
    pub total: usize,
}

/// 读取 parquet 全表，返回列名 + 数据
fn read_table_full(key: &str) -> Result<(Vec<String>, Vec<Vec<Value>>), String> {
    let path = data_dir()?.join(key);
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
    let reader = builder.build().map_err(|e| format!("解析失败: {e}"))?;
    let num_cols = columns.len();
    let mut rows = Vec::new();
    for batch in reader {
        let batch = batch.map_err(|e| format!("读取数据失败: {e}"))?;
        for i in 0..batch.num_rows() {
            rows.push(
                (0..num_cols)
                    .map(|c| array_value(batch.column(c), i))
                    .collect(),
            );
        }
    }
    Ok((columns, rows))
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

fn write_csv(path: &Path, columns: &[String], rows: &[Vec<Value>]) -> Result<(), String> {
    use std::io::{BufWriter, Write};
    // 文件开头写 UTF-8 BOM，保证 Excel 打开中文不乱码
    let file = File::create(path).map_err(|e| format!("创建 CSV 失败: {e}"))?;
    let mut buf = BufWriter::new(file);
    buf.write_all(b"\xEF\xBB\xBF")
        .map_err(|e| format!("写入 BOM 失败: {e}"))?;
    let mut wtr = csv::WriterBuilder::new().from_writer(buf);
    wtr.write_record(columns)
        .map_err(|e| format!("写入表头失败: {e}"))?;
    for row in rows {
        let rec: Vec<String> = row.iter().map(value_to_string).collect();
        wtr.write_record(&rec).map_err(|e| format!("写入行失败: {e}"))?;
    }
    wtr.flush().map_err(|e| format!("保存失败: {e}"))?;
    Ok(())
}

/// xlsx 工作表名：去扩展名、替换非法字符、截断 31 字符
fn sanitize_sheet(name: &str) -> String {
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    stem.chars()
        .map(|c| match c {
            ':' | '\\' | '/' | '?' | '*' | '[' | ']' => '_',
            c => c,
        })
        .collect::<String>()
        .chars()
        .take(31)
        .collect()
}

/// 导出用文件名/sheet 名：优先源文件名，退回 parquet 名
fn sheet_label(key: &str) -> String {
    load_manifest()
        .unwrap_or_default()
        .get(key)
        .map(|(s, _)| s.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| key.to_string())
}

/// 写 xlsx（支持多 sheet）
fn write_xlsx(
    path: &Path,
    sheets: &[(String, Vec<String>, Vec<Vec<Value>>)],
) -> Result<(), String> {
    use rust_xlsxwriter::{Workbook, Worksheet};
    use std::collections::HashSet;
    let mut workbook = Workbook::new();
    let mut used: HashSet<String> = HashSet::new();
    for (name, columns, rows) in sheets {
        // 工作表名去重：同名加序号
        let base = sanitize_sheet(name);
        let mut sheet_name = base.clone();
        let mut n = 1;
        while used.contains(&sheet_name) {
            sheet_name = format!("{base}_{n}");
            n += 1;
        }
        used.insert(sheet_name.clone());
        let mut ws = Worksheet::new();
        ws.set_name(&sheet_name)
            .map_err(|e| format!("设置工作表名失败: {e}"))?;
        for (c, col) in columns.iter().enumerate() {
            ws.write_string(0, c as u16, col)
                .map_err(|e| format!("写入表头失败: {e}"))?;
        }
        for (r, row) in rows.iter().enumerate() {
            let rr = (r + 1) as u32;
            for (c, v) in row.iter().enumerate() {
                let cc = c as u16;
                match v {
                    Value::Null => {}
                    Value::String(s) => {
                        ws.write_string(rr, cc, s).map_err(|e| format!("写入失败: {e}"))?;
                    }
                    Value::Number(n) => {
                        ws.write_number(rr, cc, n.as_f64().unwrap_or(0.0))
                            .map_err(|e| format!("写入失败: {e}"))?;
                    }
                    Value::Bool(b) => {
                        ws.write_boolean(rr, cc, *b).map_err(|e| format!("写入失败: {e}"))?;
                    }
                    _ => {}
                }
            }
        }
        workbook.push_worksheet(ws);
    }
    workbook.save(path).map_err(|e| format!("保存 xlsx 失败: {e}"))?;
    Ok(())
}

/// 导出：批量导出（每文件独立文件）或合并导出（一个 xlsx，每文件一个 sheet）
#[tauri::command]
pub fn export_files(req: ExportRequest) -> Result<ExportResult, String> {
    if req.keys.is_empty() {
        return Err("未选择文件".to_string());
    }
    let out = Path::new(&req.output_dir);
    if !out.is_dir() {
        return Err(format!("输出目录不存在: {}", req.output_dir));
    }
    let mut exported = Vec::new();

    if req.merge {
        let mut sheets = Vec::new();
        for key in &req.keys {
            let (columns, rows) = read_table_full(key)?;
            sheets.push((sheet_label(key), columns, rows));
        }
        // 合并导出文件名：可用自定义名，缺省 "合并导出.xlsx"
        let raw = req
            .file_name
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("合并导出.xlsx");
        let raw = if raw.ends_with(".xlsx") {
            raw.to_string()
        } else {
            format!("{raw}.xlsx")
        };
        let out_path = out.join(&raw);
        write_xlsx(&out_path, &sheets)?;
        exported.push(raw);
    } else {
        for key in &req.keys {
            let (columns, rows) = read_table_full(key)?;
            let stem = key.rsplit_once('.').map(|(s, _)| s).unwrap_or(key);
            match req.format.as_str() {
                "csv" => {
                    let out_path = out.join(format!("{stem}.csv"));
                    write_csv(&out_path, &columns, &rows)?;
                    exported.push(
                        out_path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string(),
                    );
                }
                "xlsx" => {
                    let out_path = out.join(format!("{stem}.xlsx"));
                    write_xlsx(&out_path, &[(sheet_label(key), columns, rows)])?;
                    exported.push(
                        out_path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string(),
                    );
                }
                "parquet" => {
                    let src = data_dir()?.join(key);
                    let out_path = out.join(key);
                    std::fs::copy(&src, &out_path)
                        .map_err(|e| format!("复制失败: {e}"))?;
                    exported.push(key.clone());
                }
                _ => return Err(format!("不支持的格式: {}", req.format)),
            }
        }
    }

    Ok(ExportResult {
        total: exported.len(),
        exported,
    })
}

/// 质量检测请求
#[derive(serde::Deserialize)]
pub struct QualityRequest {
    pub key: String,
}

/// 单个字段的质量统计
#[derive(serde::Serialize)]
pub struct FieldQuality {
    pub name: String,
    pub total: usize,
    pub non_null: usize,
    pub null_rate: f64,
    pub unique: usize,
    pub duplicates: usize,
    pub min: Option<String>,
    pub max: Option<String>,
    pub avg_len: f64,
    pub max_len: usize,
}

/// 数值感知的字符串比较：两端都可解析为数字时按数值比较，否则按字符串
fn str_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => a.cmp(b),
    }
}

/// 扫描数据，对每个字段做质量检查与统计
#[tauri::command]
pub fn scan_quality(req: QualityRequest) -> Result<Vec<FieldQuality>, String> {
    let (columns, rows) = read_table_full(&req.key)?;
    let total = rows.len();
    let mut results = Vec::new();

    for (ci, col) in columns.iter().enumerate() {
        let mut non_null = 0usize;
        let mut seen: HashSet<String> = HashSet::new();
        let mut total_len = 0usize;
        let mut max_len = 0usize;
        let mut min_val: Option<String> = None;
        let mut max_val: Option<String> = None;

        for row in &rows {
            let v = row.get(ci).cloned().unwrap_or(Value::Null);
            if let Value::String(s) = v {
                non_null += 1;
                let len = s.chars().count();
                total_len += len;
                if len > max_len {
                    max_len = len;
                }
                seen.insert(s.clone());
                if let Some(m) = &min_val {
                    if str_cmp(&s, m) == std::cmp::Ordering::Less {
                        min_val = Some(s.clone());
                    }
                } else {
                    min_val = Some(s.clone());
                }
                if let Some(m) = &max_val {
                    if str_cmp(&s, m) == std::cmp::Ordering::Greater {
                        max_val = Some(s.clone());
                    }
                } else {
                    max_val = Some(s.clone());
                }
            }
        }

        let null_rate = if total > 0 {
            (total - non_null) as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        let avg_len = if non_null > 0 {
            total_len as f64 / non_null as f64
        } else {
            0.0
        };
        let duplicates = non_null.saturating_sub(seen.len());

        results.push(FieldQuality {
            name: col.clone(),
            total,
            non_null,
            null_rate,
            unique: seen.len(),
            duplicates,
            min: min_val,
            max: max_val,
            avg_len,
            max_len,
        });
    }
    Ok(results)
}

/// 连接字段映射：副文件字段 → 主文件字段
#[derive(serde::Deserialize)]
pub struct JoinPair {
    pub sec_field: String,
    pub main_field: String,
}

/// 融合文件输入：文件 + 连接字段映射（副文件字段 → 主文件字段）
#[derive(serde::Deserialize)]
pub struct MergeFileInput {
    pub key: String,
    pub joins: Vec<JoinPair>,
}

/// 数据融合请求：主文件 + 多个副文件按映射连接
#[derive(serde::Deserialize)]
pub struct MergeRequest {
    pub main_key: String,
    pub main_join_fields: Vec<String>,
    pub secondaries: Vec<MergeFileInput>,
    pub output_name: String,
    pub join_type: String, // "inner" | "left" | "right"
}

/// 获取文件的列名（供前端选择连接字段）
#[tauri::command]
pub fn get_columns(key: String) -> Result<Vec<String>, String> {
    let (columns, _) = read_table_full(&key)?;
    Ok(columns)
}

/// join 键：拼接选中的字段值
fn join_key(row: &[Value], idx: &[usize]) -> String {
    idx.iter()
        .map(|&i| value_to_string(row.get(i).unwrap_or(&Value::Null)))
        .collect::<Vec<_>>()
        .join("\u{1f}")
}

/// 写全字符串的 parquet 到 data/
fn write_values_parquet(key: &str, columns: &[String], rows: &[Vec<Value>]) -> Result<(), String> {
    let arrays: Vec<ArrayRef> = (0..columns.len())
        .map(|i| {
            let mut b = StringBuilder::new();
            for row in rows {
                match row.get(i).unwrap_or(&Value::Null) {
                    Value::Null => b.append_null(),
                    Value::String(s) => b.append_value(s),
                    _ => b.append_null(),
                }
            }
            Arc::new(b.finish()) as ArrayRef
        })
        .collect();
    let fields: Vec<Field> = columns
        .iter()
        .map(|c| Field::new(c.clone(), ArrowDataType::Utf8, true))
        .collect();
    let schema = Arc::new(Schema::new(fields));
    let batch = RecordBatch::try_new(schema, arrays).map_err(|e| format!("构建数据失败: {e}"))?;
    let out_path = data_dir()?.join(key);
    let file = File::create(&out_path).map_err(|e| format!("无法创建文件: {e}"))?;
    let mut writer = ArrowWriter::try_new(file, batch.schema(), None)
        .map_err(|e| format!("创建写入器失败: {e}"))?;
    writer.write(&batch).map_err(|e| format!("写入失败: {e}"))?;
    writer.close().map_err(|e| format!("保存失败: {e}"))?;
    Ok(())
}

/// 数据融合：以主文件为基准 left join 多个副文件。
/// 副文件同名字段按字母顺序加 A_/B_ 前缀区分（第一个副文件 A_，第二个 B_，依此类推）。
#[tauri::command]
pub fn merge_files(req: MergeRequest) -> Result<ImportResult, String> {
    if req.main_join_fields.is_empty() {
        return Err("主文件请至少选择一个连接字段".to_string());
    }
    if req.secondaries.is_empty() {
        return Err("请至少选择一个副文件".to_string());
    }
    let join_type = req.join_type.as_str();
    if !matches!(join_type, "inner" | "left" | "right") {
        return Err("连接方式必须为 inner / left / right".to_string());
    }

    let (main_cols, main_rows) = read_table_full(&req.main_key)?;
    let mut result_cols: Vec<String> = main_cols.clone();
    let mut used: HashSet<String> = main_cols.iter().cloned().collect();

    struct SecData {
        fields: Vec<(usize, String)>, // (副文件列索引, 结果字段名)
        rows: Vec<Vec<Value>>,
        map: HashMap<String, usize>, // 副 join 键 -> 行
        main_idx: Vec<usize>,        // 对应主文件列索引（按映射顺序）
    }
    let mut secs: Vec<SecData> = Vec::new();

    for (i, sec_req) in req.secondaries.iter().enumerate() {
        let prefix = (b'A' + i as u8) as char;
        let (sec_cols, sec_rows) = read_table_full(&sec_req.key)?;
        if sec_req.joins.is_empty() {
            return Err(format!("副文件 {} 请至少配置一个连接字段", sec_req.key));
        }
        let sec_idx: Vec<usize> = sec_req
            .joins
            .iter()
            .map(|j| {
                sec_cols
                    .iter()
                    .position(|c| c == &j.sec_field)
                    .ok_or_else(|| format!("副文件 {} 缺少字段: {}", sec_req.key, j.sec_field))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let main_idx: Vec<usize> = sec_req
            .joins
            .iter()
            .map(|j| {
                main_cols
                    .iter()
                    .position(|c| c == &j.main_field)
                    .ok_or_else(|| format!("主文件缺少字段: {}", j.main_field))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let join_sec: HashSet<String> =
            sec_req.joins.iter().map(|j| j.sec_field.clone()).collect();
        let mut fields = Vec::new();
        for (ci, col) in sec_cols.iter().enumerate() {
            if join_sec.contains(col) {
                continue;
            }
            let res_name = if used.contains(col) {
                let mut candidate = format!("{prefix}_{col}");
                let mut n = 1;
                while used.contains(&candidate) {
                    n += 1;
                    candidate = format!("{prefix}_{n}_{col}");
                }
                candidate
            } else {
                col.clone()
            };
            used.insert(res_name.clone());
            fields.push((ci, res_name));
        }
        result_cols.extend(fields.iter().map(|(_, n)| n.clone()));

        let mut map: HashMap<String, usize> = HashMap::new();
        for (ri, row) in sec_rows.iter().enumerate() {
            map.entry(join_key(row, &sec_idx)).or_insert(ri);
        }
        secs.push(SecData {
            fields,
            rows: sec_rows,
            map,
            main_idx,
        });
    }

    // 结果行总宽：主字段 + 各副字段
    let total_len = main_cols.len() + secs.iter().map(|s| s.fields.len()).sum::<usize>();
    let mut offsets = Vec::new();
    let mut off = main_cols.len();
    for s in &secs {
        offsets.push(off);
        off += s.fields.len();
    }

    // 主文件逐行连接
    let mut result_rows: Vec<Vec<Value>> = Vec::with_capacity(main_rows.len());
    for main_row in &main_rows {
        let mut out: Vec<Value> = vec![Value::Null; total_len];
        for (ci, v) in main_row.iter().enumerate() {
            out[ci] = v.clone();
        }
        let mut all_match = true;
        for (si, sec) in secs.iter().enumerate() {
            let key = join_key(main_row, &sec.main_idx);
            if let Some(&ri) = sec.map.get(&key) {
                for (fi, &(sec_col, _)) in sec.fields.iter().enumerate() {
                    out[offsets[si] + fi] =
                        sec.rows[ri].get(sec_col).cloned().unwrap_or(Value::Null);
                }
            } else {
                all_match = false;
            }
        }
        // inner：副文件任一未匹配则丢弃该主行
        if join_type == "inner" && !all_match {
            continue;
        }
        result_rows.push(out);
    }

    // right：追加副文件中未匹配主文件的行（主字段为 null）
    if join_type == "right" {
        for (si, sec) in secs.iter().enumerate() {
            let main_keys: HashSet<String> = main_rows
                .iter()
                .map(|r| join_key(r, &sec.main_idx))
                .collect();
            for (sec_key, &ri) in sec.map.iter() {
                if !main_keys.contains(sec_key) {
                    let mut out: Vec<Value> = vec![Value::Null; total_len];
                    for (fi, &(sec_col, _)) in sec.fields.iter().enumerate() {
                        out[offsets[si] + fi] =
                            sec.rows[ri].get(sec_col).cloned().unwrap_or(Value::Null);
                    }
                    result_rows.push(out);
                }
            }
        }
    }

    // 写 parquet + 注册 manifest
    let out_name = {
        let n = req.output_name.trim();
        let base = if n.is_empty() { "融合结果" } else { n };
        if base.ends_with(".parquet") {
            base.to_string()
        } else {
            format!("{base}.parquet")
        }
    };
    write_values_parquet(&out_name, &result_cols, &result_rows)?;
    let _ = update_manifest(&out_name, &out_name, "");

    Ok(ImportResult {
        key: out_name.clone(),
        source: out_name,
        sheet: String::new(),
        columns: result_cols,
        rows: vec![],
        row_count: result_rows.len(),
    })
}

/// 全局设置：主题 + 默认值，持久化到 app_config_dir/settings.json
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// 主题："light" | "dark"
    pub theme: String,
    /// 表格默认分页大小
    pub page_size: usize,
    /// 工作台快捷入口的功能 key 列表；为空则显示全部
    pub quick_entries: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "light".to_string(),
            page_size: 10,
            quick_entries: vec![],
        }
    }
}

fn config_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("无法定位配置目录: {e}"))?;
    Ok(dir.join("settings.json"))
}

#[tauri::command]
pub fn get_config(app: tauri::AppHandle) -> Result<AppSettings, String> {
    let p = config_path(&app)?;
    if !p.exists() {
        return Ok(AppSettings::default());
    }
    let s = std::fs::read_to_string(&p).map_err(|e| format!("读取配置失败: {e}"))?;
    serde_json::from_str(&s).map_err(|e| format!("配置格式错误: {e}"))
}

#[tauri::command]
pub fn save_config(app: tauri::AppHandle, req: AppSettings) -> Result<(), String> {
    let p = config_path(&app)?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建配置目录失败: {e}"))?;
    }
    let s = serde_json::to_string_pretty(&req).map_err(|e| e.to_string())?;
    std::fs::write(&p, s).map_err(|e| format!("保存配置失败: {e}"))
}

/// 窗口主题请求
#[derive(serde::Deserialize)]
pub struct WindowTheme {
    pub dark: bool,
}

// 切换 Windows 系统标题栏（关闭/最小化/最大化区域）明暗。
// 不依赖 Tauri 的 set_theme（Windows 上部分支持），直接用
// DwmSetWindowAttribute(DWMWA_USE_IMMERSIVE_DARK_MODE) 硬切。
#[cfg(windows)]
#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: *const core::ffi::c_void,
        attr: u32,
        pv: *const core::ffi::c_void,
        cb: u32,
    ) -> i32;
}

#[tauri::command]
pub fn set_window_theme(window: tauri::Window, req: WindowTheme) -> Result<(), String> {
    #[cfg(windows)]
    {
        // HWND 是 `#[repr(transparent)]` 的裸指针包装，取 .0 传给 DwmSetWindowAttribute
        let hwnd = window.hwnd().map_err(|e| format!("获取窗口句柄失败: {e}"))?;
        // DWMWA_USE_IMMERSIVE_DARK_MODE = 20
        let dark: i32 = if req.dark { 1 } else { 0 };
        unsafe {
            DwmSetWindowAttribute(
                hwnd.0 as *const core::ffi::c_void,
                20,
                &dark as *const i32 as *const core::ffi::c_void,
                std::mem::size_of::<i32>() as u32,
            );
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (window, req);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    /// 数据融合：主文件 left join 多个副文件，同名字段加前缀
    #[test]
    fn merge_files_works() {
        let m = std::env::temp_dir().join("rvt_merge_main.csv");
        let mut f = File::create(&m).unwrap();
        f.write_all(b"id,name\n1,Alice\n2,Bob\n").unwrap();
        import_file(m.to_string_lossy().to_string()).unwrap();

        let a = std::env::temp_dir().join("rvt_merge_a.csv");
        let mut f = File::create(&a).unwrap();
        f.write_all(b"id,name,extra\n1,Al,X\n2,Bo,Y\n").unwrap();
        import_file(a.to_string_lossy().to_string()).unwrap();

        let b = std::env::temp_dir().join("rvt_merge_b.csv");
        let mut f = File::create(&b).unwrap();
        f.write_all(b"id,name\n1,A1\n2,B2\n").unwrap();
        import_file(b.to_string_lossy().to_string()).unwrap();

        let res = merge_files(MergeRequest {
            main_key: "rvt_merge_main.parquet".to_string(),
            main_join_fields: vec!["id".to_string()],
            secondaries: vec![
                MergeFileInput {
                    key: "rvt_merge_a.parquet".to_string(),
                    joins: vec![JoinPair {
                        sec_field: "id".to_string(),
                        main_field: "id".to_string(),
                    }],
                },
                MergeFileInput {
                    key: "rvt_merge_b.parquet".to_string(),
                    joins: vec![JoinPair {
                        sec_field: "id".to_string(),
                        main_field: "id".to_string(),
                    }],
                },
            ],
            output_name: "融合测试".to_string(),
            join_type: "left".to_string(),
        })
        .unwrap();

        // 主 id,name + 副a name(冲突→A_name)/extra + 副b name(冲突→B_name)
        assert_eq!(
            res.columns,
            vec!["id", "name", "A_name", "extra", "B_name"]
        );
        assert_eq!(res.row_count, 2);

        // 读回验证数据
        let back = read_parquet(ReadRequest {
            key: "融合测试.parquet".to_string(),
            offset: 0,
            limit: 10,
            filters: vec![],
        })
        .unwrap();
        assert_eq!(
            back.rows[0],
            vec![json!("1"), json!("Alice"), json!("Al"), json!("X"), json!("A1")]
        );
        assert_eq!(
            back.rows[1],
            vec![json!("2"), json!("Bob"), json!("Bo"), json!("Y"), json!("B2")]
        );

        for p in ["rvt_merge_main.parquet", "rvt_merge_a.parquet", "rvt_merge_b.parquet", "融合测试.parquet"] {
            let _ = std::fs::remove_file(data_dir().unwrap().join(p));
        }
        let _ = std::fs::remove_file(&m);
        let _ = std::fs::remove_file(&a);
        let _ = std::fs::remove_file(&b);
    }

    /// 质量检测：统计每个字段的非空、空值率、唯一值、极值
    #[test]
    fn scan_quality_works() {
        let tmp = std::env::temp_dir().join("rvt_scan.csv");
        let mut f = File::create(&tmp).unwrap();
        f.write_all(b"name,age\nAlice,25\nBob,30\nCharlie,\n").unwrap();
        import_file(tmp.to_string_lossy().to_string()).unwrap();

        let res = scan_quality(QualityRequest {
            key: "rvt_scan.parquet".to_string(),
        })
        .unwrap();
        assert_eq!(res.len(), 2);

        let name = res.iter().find(|f| f.name == "name").unwrap();
        assert_eq!(name.total, 3);
        assert_eq!(name.non_null, 3);
        assert_eq!(name.null_rate, 0.0);
        assert_eq!(name.unique, 3);
        assert_eq!(name.duplicates, 0);
        assert_eq!(name.avg_len, 5.0); // Alice(5)+Bob(3)+Charlie(7) / 3
        assert_eq!(name.max_len, 7);
        assert!(name.min.is_some());

        let age = res.iter().find(|f| f.name == "age").unwrap();
        assert_eq!(age.total, 3);
        assert_eq!(age.non_null, 2);
        assert!(age.null_rate > 30.0, "age 空值率应 >30%，实际 {}", age.null_rate);
        assert_eq!(age.max_len, 2); // "25" / "30" 长度均为 2

        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(data_dir().unwrap().join("rvt_scan.parquet"));
    }

    /// 导出功能：批量导出 csv + 合并导出 xlsx
    #[test]
    fn export_files_works() {
        let tmp = std::env::temp_dir().join("rvt_export_src.csv");
        let mut f = File::create(&tmp).unwrap();
        f.write_all(b"name,age\nAlice,25\nBob,30\n").unwrap();
        import_file(tmp.to_string_lossy().to_string()).unwrap();

        let out = std::env::temp_dir().join("rvt_export_out");
        std::fs::create_dir_all(&out).unwrap();

        // 批量导出 csv
        let res = export_files(ExportRequest {
            keys: vec!["rvt_export_src.parquet".to_string()],
            format: "csv".to_string(),
            output_dir: out.to_string_lossy().to_string(),
            merge: false,
            file_name: None,
        })
        .unwrap();
        assert_eq!(res.total, 1);
        let csv_path = out.join("rvt_export_src.csv");
        assert!(csv_path.exists());
        // 校验 UTF-8 BOM（Excel 兼容）
        let head = std::fs::read(&csv_path).unwrap();
        assert_eq!(&head[..3], b"\xEF\xBB\xBF", "CSV 缺少 UTF-8 BOM");

        // 合并导出 xlsx（每文件一个 sheet）
        let res2 = export_files(ExportRequest {
            keys: vec!["rvt_export_src.parquet".to_string()],
            format: "xlsx".to_string(),
            output_dir: out.to_string_lossy().to_string(),
            merge: true,
            file_name: None,
        })
        .unwrap();
        assert_eq!(res2.total, 1);
        assert!(out.join("合并导出.xlsx").exists());

        // 自定义合并导出文件名
        let res3 = export_files(ExportRequest {
            keys: vec!["rvt_export_src.parquet".to_string()],
            format: "xlsx".to_string(),
            output_dir: out.to_string_lossy().to_string(),
            merge: true,
            file_name: Some("汇总数据".to_string()),
        })
        .unwrap();
        assert_eq!(res3.total, 1);
        assert!(out.join("汇总数据.xlsx").exists());

        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_dir_all(&out);
        let _ = std::fs::remove_file(data_dir().unwrap().join("rvt_export_src.parquet"));
    }

    /// 合并导出时同名源文件的工作表名应去重
    #[test]
    fn xlsx_sheet_name_dedup() {
        let out = std::env::temp_dir().join("rvt_dedup.xlsx");
        let sheets = vec![
            ("note.xlsx".to_string(), vec!["a".to_string()], vec![]),
            ("note.xlsx".to_string(), vec!["b".to_string()], vec![]),
        ];
        write_xlsx(&out, &sheets).unwrap();
        assert!(out.exists());
        let _ = std::fs::remove_file(&out);
    }

    /// 删除 parquet 文件时应同步清理 manifest 记录
    #[test]
    fn delete_cache_cleans_manifest() {
        let key = "test_delete.parquet";
        let p = data_dir().unwrap().join(key);
        std::fs::write(&p, b"dummy").unwrap();
        update_manifest(key, "test.csv", "Sheet1").unwrap();
        assert!(load_manifest().unwrap().contains_key(key));

        delete_cache(key.to_string()).unwrap();

        assert!(!p.exists());
        assert!(!load_manifest().unwrap().contains_key(key));
    }

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
        // 读取统一为字符串，不依赖类型推断
        assert_eq!(
            back.rows[0],
            vec![json!("Alice"), json!("25"), json!("90.5")]
        );
        assert_eq!(back.rows[1], vec![json!("Bob"), json!("30"), Value::Null]);

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
