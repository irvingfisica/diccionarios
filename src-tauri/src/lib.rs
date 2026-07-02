use serde::{Serialize,Deserialize};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, CACHE_CONTROL, PRAGMA, USER_AGENT};
use std::time::Duration;
use std::path::{Path,PathBuf};
use std::io::{Read,BufReader,Cursor};
use std::collections::{HashMap,BTreeMap};
use futures::stream::{self, StreamExt};
use tauri::State;
use tauri::Manager;
use std::sync::Mutex;
use polars::prelude::*;
use serde_json::Value;
use std::fs::File;
use chardetng::{EncodingDetector,Iso2022JpDetection, Utf8Detection};

fn cliente_ckan() -> Result<reqwest::Client, String> {
    let mut headers = HeaderMap::new();

    headers.insert(USER_AGENT, HeaderValue::from_static(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
             AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/122.0.0.0 Safari/537.36"));

    headers.insert(
        ACCEPT,
        HeaderValue::from_static(
            "application/json, text/plain, */*"
        ),
    );

    headers.insert(
        ACCEPT_LANGUAGE,
        HeaderValue::from_static(
            "es-MX,es;q=0.9,en;q=0.8"
        ),
    );

    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("no-cache"),
    );

    headers.insert(
        PRAGMA,
        HeaderValue::from_static("no-cache"),
    );

    reqwest::Client::builder().default_headers(headers)
        .gzip(true)
        .brotli(true)
        .deflate(true)
        .tcp_keepalive(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .build().map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct CkanResponse<T> {
    success: bool,
    result: T
}

#[derive(Debug, Deserialize)]
struct PackageSearchResponse {
    count: usize,
    results: Vec<Conjunto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Institucion {
    pub id: String,
    pub name: String,
    pub display_name: String
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheInstituciones {
    pub instituciones: Vec<Institucion>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conjunto {
    pub id: String,
    pub name: String,
    pub title: String,
    #[serde(default)]
    pub institucion: String,
    pub resources: Vec<Resource>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    id: String,
    name: String,
    description: String,
    url: String,
}

pub struct ContenedorDatos {
    pub dataframe: Mutex<Option<DataFrame>>,
    pub dicchelp: Mutex<Option<DataFrame>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum TipoColumna {
    Numero,
    Coordenada,
    Fecha,
    Texto,
}

impl TipoColumna {
    pub fn to_polartype(&self) -> DataType {
        match self {
            TipoColumna::Numero => DataType::Float64,
            TipoColumna::Fecha => DataType::Date,
            TipoColumna::Texto => DataType::String,
            TipoColumna::Coordenada => DataType::Float64
        }
    }

    pub fn from_polartype(dt: &DataType, coord: bool) -> Self {
        match dt {
            DataType::Float64 => {
                if coord {TipoColumna::Coordenada } else { TipoColumna::Numero}}
            DataType::Int64 => {
                if coord {TipoColumna::Coordenada } else { TipoColumna::Numero}}
            DataType::Date => TipoColumna::Fecha,
            _ => TipoColumna::Texto
        }
    }
}

#[derive(Serialize)]
pub struct Reporte {
    pub total_filas: usize,
    pub columnas: Vec<String>,
    pub esquema: BTreeMap<String, TipoColumna>,
}

#[derive(Serialize)]
struct DiccionarioFila {
    columna: Option<String>,
    tipo: Option<String>,
    etiqueta: Option<String>,
    descripcion: Option<String>
}

#[derive(Serialize, Deserialize)]
struct FieldInfo {
    label: Option<String>,
    notes: Option<String>,
    type_override: Option<String>
}

#[derive(Serialize, Deserialize)]
struct DatastoreField {
    id: Option<String>,
    #[serde(rename = "type")]
    field_type: Option<String>,
    info: FieldInfo
}

#[derive(Serialize, Deserialize)]
struct DatastoreRequest {
    resource_id: String,
    force: bool,
    fields: Vec<DatastoreField>
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(ContenedorDatos {
            dataframe: Mutex::new(None),
            dicchelp: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            obtener_instituciones,
            obtener_conjuntos,
            leer_base,
            leer_dicc,
            obtener_bloque,
            enviar_datos
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
async fn enviar_datos(apikey: String, datos: DatastoreRequest) -> Result<Value, String> {
    let cliente = cliente_ckan()?;

    let respuesta = cliente.post("https://www.datos.gob.mx/api/3/action/datastore_create")
            .header("Authorization", apikey)
            .json(&datos)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;

            let valor: Value = respuesta
            .json()
            .await
            .map_err(|e| e.to_string())?;

        Ok(valor)
}

#[tauri::command]
async fn leer_dicc(ruta: String, state: State<'_, ContenedorDatos>) -> Result<Vec<DiccionarioFila>, String> {
    let path = Path::new(&ruta);
    if !path.exists() {
        return Err("El archivo no existe.".to_string());
    }

    let file = File::open(&ruta).map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(file);
    let mut buffer_inicio = vec![0; 4096];
    let bytes_leidos =  reader.read(&mut buffer_inicio).map_err(|e| e.to_string())?;

    let mut file_completo = File::open(&ruta).map_err(|e| e.to_string())?;
    let mut bytes_puros = Vec::new();
    file_completo.read_to_end(&mut bytes_puros).map_err(|e| e.to_string())?;

    let (texto_convertido, _encod, tuviera_errores) = encoding_rs::UTF_8.decode(&bytes_puros);

    let contenido_final = if tuviera_errores {
        let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
        detector.feed(&buffer_inicio[..bytes_leidos], true);
        let encoding_alternativo = detector.guess(None, Utf8Detection::Allow);

        let (texto_alt, _encalt, _err) = encoding_alternativo.decode(&bytes_puros);

        texto_alt.into_owned()
    } else {
        texto_convertido.into_owned()
    };

    let cursor = Cursor::new(contenido_final);

    let df = CsvReader::new(cursor)
        .with_options(
            CsvReadOptions::default()
                .with_has_header(true)
        )
        .finish()
        .map_err(|e| e.to_string())?;

    let columnas = df.select_at_idx(0).unwrap().str().map_err(|e| e.to_string())?;
    let tipos = df.select_at_idx(1).unwrap().str().map_err(|e| e.to_string())?;
    let etiquetas = df.select_at_idx(2).unwrap().str().map_err(|e| e.to_string())?;
    let descripciones = df.select_at_idx(3).unwrap().str().map_err(|e| e.to_string())?;

    let mut salida = Vec::with_capacity(df.height());

    for i in 0..df.height() {
        salida.push(DiccionarioFila {
            columna: columnas.get(i).map(|s| s.to_string()),
            tipo: tipos.get(i).map(|s| s.to_string()),
            etiqueta: etiquetas.get(i).map(|s| s.to_string()),
            descripcion: descripciones.get(i).map(|s| s.to_string()),
        });
    }

    *state
        .dicchelp
        .lock()
        .map_err(|_| "Error al bloquear estado")? = Some(df);

    Ok(salida)
}

#[tauri::command]
async fn leer_base(url: String, state: State<'_, ContenedorDatos>) -> Result<Reporte, String> {
    let cliente = cliente_ckan()?;

    let bytes = cliente
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    let mut header_cursor = Cursor::new(bytes.clone());

    let extractor_headers = CsvReader::new(&mut header_cursor).with_options(
            CsvReadOptions::default()
            .with_has_header(true)
            .with_n_rows(Some(0))
            .with_infer_schema_length(Some(0))
        );

    let df_headers = extractor_headers.finish().map_err(|e| format!("Error al mapear columnas: {}", e))?;
    let ncols = df_headers.width();
    let types = vec![DataType::String;ncols];

    let cursor = Cursor::new(bytes);

    let mut df = CsvReader::new(cursor)
        .with_options(
            CsvReadOptions::default()
            .with_has_header(true)
            .with_dtype_overwrite(Some(Arc::new(types)))
        )
        .finish()
        .map_err(|e| e.to_string())?;

    df = castear_frame(df, TipoColumna::Fecha);
    df = castear_frame(df, TipoColumna::Numero);

    let mut esquema = BTreeMap::new();
    for col in df.columns() {
        let nombre_columna = col.name().to_string();
        let tipo_final = TipoColumna::from_polartype(col.dtype(), false);
        esquema.insert(nombre_columna, tipo_final);
    }

    let columnas: Vec<String> = df.get_column_names().iter().map(|s| s.to_string()).collect();
    let total_filas = df.height();

    *state
        .dataframe
        .lock()
        .map_err(|_| "Error al bloquear estado")? = Some(df);

    Ok(Reporte { total_filas, columnas, esquema })
}

#[tauri::command]
async fn obtener_instituciones() -> Result<Vec<Institucion>, String> {
    let cliente = cliente_ckan()?;
    let lista_actual = obtener_lista_instituciones(&cliente).await?;

    let mut cache = cargar_cache();

    let mapa: HashMap<String, Institucion> = cache.instituciones.iter().cloned().map(|x| (x.name.clone(), x)).collect();

    let faltantes: Vec<String> = lista_actual.iter().filter(|nombre| !mapa.contains_key(*nombre)).cloned().collect();

    let nuevas: Vec<Institucion> = stream::iter(faltantes).map(|nombre| {
        let cliente = cliente.clone();
        async move {
        obtener_detalle_institucion(&nombre, &cliente).await}
    }).buffer_unordered(10).filter_map(|x| async move {
        match x {
            Ok(i) => Some(i),
            Err(e) => {
                eprintln!("Error: {}", e);
                None
            }
        }
    }).collect().await;

    for institucion  in nuevas {
            cache.instituciones.push(institucion);
    }

    cache.instituciones.retain(|i| {lista_actual.contains(&i.name)});

    guardar_cache(&cache)?;

    let mapa_final: HashMap<String, Institucion> = cache.instituciones.iter().cloned().map(|x| (x.name.clone(), x)).collect();

    let mut resultado = Vec::new();

    for nombre in lista_actual {
        if let Some(i) = mapa_final.get(&nombre) {
            resultado.push(i.clone());
        }
    }

    resultado.sort_by(|a,b| {
        a.display_name.cmp(&b.display_name)
    });

    Ok(resultado)
}

#[tauri::command]
async fn obtener_conjuntos(institucion: String) -> Result<Vec<Conjunto>, String> {
    let cliente = cliente_ckan()?;
    let conjuntos = obtener_conjuntos_institucion(&institucion, &cliente).await?;

    Ok(conjuntos)
}

async fn obtener_lista_instituciones(cliente: &reqwest::Client) -> Result<Vec<String>, String> {

    let respuesta= cliente.get("https://www.datos.gob.mx/api/3/action/organization_list").send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?;

    let datos: CkanResponse<Vec<String>> = respuesta.json().await.map_err(|e| e.to_string())?;

    if !datos.success {return Err("CKAN regresó success=false".to_string());}

    Ok(datos.result)
}

fn ruta_cache() -> Result<PathBuf, String> {
    let mut path = dirs::cache_dir().ok_or("No se pudo localizar el directorio de cache")?;

    path.push("validador_app");
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;

    path.push("instituciones.json");

    Ok(path)
}

fn cargar_cache() -> CacheInstituciones {
    let ruta = match ruta_cache() {
        Ok(x) => x,
        Err(_) => return CacheInstituciones::default()
    };

    if !ruta.exists() {
        return CacheInstituciones::default();
    }

    let contenido = match std::fs::read_to_string(ruta) {
        Ok(x) => x,
        Err(_) => return CacheInstituciones::default(),
    };

    serde_json::from_str(&contenido).unwrap_or_default()
}

fn guardar_cache(cache: &CacheInstituciones) -> Result<(), String> {
    let ruta = ruta_cache()?;

    let json = serde_json::to_string_pretty(cache).map_err(|e| e.to_string())?;

    std::fs::write(ruta, json).map_err(|e| e.to_string())?;

    Ok(())
}

async fn obtener_detalle_institucion(nombre: &str, cliente: &reqwest::Client) -> Result<Institucion, String> {

    let respuesta = cliente
    .get(
        "https://www.datos.gob.mx/api/3/action/organization_show"
    )
    .query(&[("id", nombre)]).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?;

    let datos: CkanResponse<Institucion> =  respuesta.json().await.map_err(|e| e.to_string())?;

    if !datos.success {return Err("CKAN regresó success=false".to_string());}

    Ok(datos.result)
}

async fn obtener_conjuntos_institucion(institucion: &str, cliente: &reqwest::Client) -> Result<Vec<Conjunto>, String> {
    let respuesta = cliente
    .get(
        "https://www.datos.gob.mx/api/3/action/package_search"
    )
    .query(&[
        ("fq",format!("organization:{institucion}")),("rows","1000".to_string())
    ]).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?;

    let datos: CkanResponse<PackageSearchResponse> = respuesta.json().await.map_err(|e| e.to_string())?;

    if !datos.success {return Err("CKAN regresó success=false".to_string());}

    let mut conjuntos = datos.result.results;

    for conjunto in &mut conjuntos {
        conjunto.institucion = institucion.to_string();
    }

    Ok(conjuntos)
}

fn castear_frame(df: DataFrame, tipo: TipoColumna) -> DataFrame {
    let columns: Vec<String> = df.get_column_names().iter().map(|name| name.to_string()).collect();

    let mut temporal = df;

    for nombre in columns {
        temporal = castear_columna(temporal, &nombre, tipo);
    }

    temporal
}

fn castear_columna(df: DataFrame, columna: &str, tipo: TipoColumna) -> DataFrame {

    match tipo {
        TipoColumna::Fecha => {

            let columna_original = match df.column(columna) {
                Ok(c) => c,
                Err(_) => return df, 
            };

            let nulos_originales = columna_original.null_count();
            let total = df.height();

            if nulos_originales == total {
                return df;
            }

            for formato in ["%Y-%m-%d", "%d-%m-%Y","%Y/%m/%d", "%d/%m/%Y"] {
                let expr = col(columna).str().to_date(StrptimeOptions {
                format: Some(formato.into()),
                strict: true,
                exact: true,
                cache: true,
            });
                match df.clone().lazy().with_column(expr).collect() {
                    Ok(df_transformado) => {
                        let nulos_nuevos = df_transformado.column(columna)
                            .map(|c| c.null_count())
                            .unwrap_or(0);

                            if nulos_nuevos == nulos_originales {
                            return df_transformado;
                        }
                        continue;
                    },
                    Err(_) => continue
                }
            };

            return df;
        },
        TipoColumna::Numero => {

            if df.column(columna).map(|c| c.dtype() == &DataType::Date).unwrap_or(false) { return df; }

            let columna_actual = match df.column(columna) {
                Ok(c) => c,
                Err(_) => return df,
            };

            if tiene_ceros_iniciales(columna_actual) {
                return df;
            }

            let tipo_destino = if tiene_decimales(columna_actual) {
                DataType::Float64
            } else {
                DataType::Int64
            };

            let expr = col(columna).strict_cast(tipo_destino);

            match df.clone().lazy().with_column(expr).collect() {
                Ok(df_transformado) => return df_transformado,
                Err(_) => return df,
            }
        },
        _ => {
            let expr = col(columna).strict_cast(tipo.to_polartype());
                match df.clone().lazy().with_column(expr).collect() {
                    Ok(df_transformado) => return df_transformado,
                    Err(_) => return df
                }
        }
    }

}

fn tiene_ceros_iniciales(col: &Column) -> bool {
    if col.dtype() != &DataType::String {
        return false;
    }

    let Ok(ca) = col.str() else {
        return false;
    };

    for txt in ca.into_iter().flatten() {
        let txt = txt.trim();

        if txt.len() > 1
            && txt.starts_with('0')
            && txt.chars().all(|c| c.is_ascii_digit())
        {
            return true;
        }
    }

    false
}

fn tiene_decimales(col: &Column) -> bool {
        let Ok(ca) = col.str() else {
        return false;
    };

    ca.into_iter().flatten().any(|txt| {
        let txt = txt.trim();

        !txt.is_empty()
            && txt.contains('.')
    })
}

#[tauri::command]
async fn obtener_bloque(start_row: i64, page_size: i64,state: State<'_, ContenedorDatos>) -> Result<Value, String> {
    let guardado = state.dataframe.lock().map_err(|_| "Error al bloquear el estado")?;

    let df = guardado.as_ref().ok_or("No hay dataframe")?;

    let df_bloque = df.clone().slice(start_row, page_size as usize);

    let mut buf = Vec::new();
    JsonWriter::new(&mut buf).with_json_format(JsonFormat::Json).finish(&mut df_bloque.clone()).map_err(|e| format!("Error de formato al escribir JSON: {}", e))?;

    let rows: Value = serde_json::from_slice(&buf).map_err(|e| format!("Error al estructurar el JSON: {}", e))?;

    Ok(rows)
}