import { save } from '@tauri-apps/plugin-dialog';
import { getCurrentWindow } from '@tauri-apps/api/window';
import 'bootstrap/dist/css/bootstrap.min.css';
import * as bootstrap from 'bootstrap/dist/js/bootstrap.bundle.min.js';
import "@tarekraafat/autocomplete.js/dist/css/autoComplete.02.css";
import autoComplete from "@tarekraafat/autocomplete.js";
import * as d3 from 'd3';
import { AllCommunityModule, ModuleRegistry, createGrid } from 'ag-grid-community';
ModuleRegistry.registerModules([AllCommunityModule]);
const { invoke } = window.__TAURI__.core;
import { Toast } from 'bootstrap';

let instituciones = await invoke("obtener_instituciones");

window.appState = {
  grid: null,
  datos: null
};

const acjs = new autoComplete({
    selector: "#buscainst",
    placeHolder: "Busca una institución...",
    data: {
        src: instituciones,
        keys: ["display_name"]
    },
    diacritics: true,
    threshold: 1,
    debounce: 150,
    resultList: {
        maxResults: 10,
        noResults: true
    },
    resultItem: {
        highlight: true
    },
    events: {
        input: {
            selection(event) {
                event.preventDefault();
                const item = event.detail.selection.value;
                acjs.input.value = item.display_name;
                console.log(item);
                obtenerConjuntos(item.name,item.display_name);
            },
        },
    },
});

async function obtenerConjuntos(name,institucion) {
    let conjuntos = await invoke("obtener_conjuntos",{institucion: name});

    const recursos = d3.select("#recursos");
    recursos.selectAll("*").remove();

    recursos.append("p").html("Selecciona un conjunto y en su interior el recurso con el cual comparar la base de datos que estás limpiando. Si tu base de datos es demasiado grande ten paciencia.");

    const items = recursos.append("div").attr("class","accordion")
        .attr("id","recursos_acc")
        .selectAll(".accordion-item").data(conjuntos)
        .join("div").attr("class","accordion-item");

    items.append("h3").attr("class","accordion-header")
        .append("button").attr("class","accordion-button collapsed")
        .attr("type","button")
        .attr("data-bs-toggle","collapse")
        .attr("data-bs-target",(d,i) => "#coll" + (i+1))
        .html(d => d.title);

    items.append("div").attr("id",(d,i) => "coll" + (i+1))
        .attr("class","accordion-collapse collapse")
        .attr("data-bs-parent","#recursos_acc")
        .append("div")
        .attr("class","accordion-body bg-body-tertiary")
        .append("table")
        .attr("class","table table-hover")
        .append("tbody")
        .selectAll("tr").data(d => d.resources.map(r => ({...r,conjunto: d.title})))
        .join("tr").append("td").html(p => p.name)
        .style("cursor","pointer")
        .on("click",async (e,p)  => {
            let datos = await invoke("leer_base",{url:p.url});
            datos.recurso = p;
            datos.institucion = institucion;
            window.appState.datos = datos;
            console.log(datos);
            await proceso(datos);
        });
        
}

const input = document.querySelector("#buscainst");

input.addEventListener("focus", function () {this.value = "";});

async function proceso(datos) {
    const recursos = d3.select("#recursos");
    recursos.selectAll("*").remove();

    const muestra = await invoke("obtener_bloque",{startRow: 0, pageSize: 10});
    console.log(muestra);

    const info = recursos.append("div").attr("id","info");
    info.append("h3").html("Base seleccionada")
    info.append("p").attr("class","mb-1").html("<strong>Institución: </strong>" + datos.institucion);
    info.append("p").attr("class","mb-1").html("<strong>Conjunto: </strong>" + datos.recurso.conjunto);
    info.append("p").attr("class","mb-1").html("<strong>Recurso: </strong>" + datos.recurso.name);

    const mesa = recursos.append("div").attr("class","row mt-5 mb-5");
    mesa.append("div").attr("class","col-md-12").attr("id","grid");
    mostrarGrid("#grid",datos);

    const instrucciones = mesa.append("div").attr("class","row mt-5");
    instrucciones.append("h3").html("Creación de diccionario");

    const t0 = instrucciones.append("div").attr("class","col-md-6")
    t0.append("p").html("Para cada columna debes de introducir los datos del diccionario.");
    t0.append("p").html("La herramienta prellena algunos de estos campos con información de la base y sugerencias.");
    t0.append("p").html("Revisa la redacción de cada campo. Las etiquetas deben ser textos cortos que sirvan para nombrar la columna en lenguaje claro. Las descripciones deben ser claras y apoyar a la interpretación de la información contenida en la columna");

    instrucciones.append("div").attr("class","col-md-1");

    const t1 = instrucciones.append("div").attr("class","col-md-5");
    t1.append("p").html("Puedes arrastrar un archivo con la información para llenar más rápido.");
    t1.append("div").attr("id","dropZone").attr("class","drop-zone").append("p")
      .html("Tu archivo debe tener formato CSV y contener en las primeras 4 columnas la información en el siguiente orden: nombres de columnas, tipos de datos, etiquetas, descripciones");
    t1.append("div").attr("class","mt-2").attr("id","copiar_todo");


    const tablero = mesa.append("div").attr("class","row mt-5");
    tablero.append("hr");

    const filas = tablero.selectAll(".fila").data(datos.columnas).join("div").attr("class","row fila");

    const left = filas.append("div").attr("class","col-md-6 mb-3 left");
    filas.append("div").attr("class","col-md-1");
    const right = filas.append("div").attr("class","col-md-5 mb-3 right");

    left.append("p").html(d => "Columna: <strong>" + d + "</strong>");

    const tipos = {Texto:"text",Numero:"numeric",Fecha:"timestamp"};

    left.append("label").attr("for",(d,i) => "tipo_" + i).attr("class","form-label  mt-3").html("Tipo de datos:");
    const selecto = left.append("select").attr("class","form-select").attr("id",(d,i) => "tipo_" + i);
    selecto.each(function(d) {

      d3.select(this)
        .selectAll("option")
        .data(["Texto","Numero","Fecha"])
        .join("option")
        .attr("value",p => tipos[p])
        .property("selected",p => datos.esquema[d] == p)
        .html(p => p)
    })

    left.append("label").attr("for",(d,i) => "etiqueta_" + i).attr("class","form-label mt-3").html("Etiqueta:");
    left.append("input").attr("type","text").attr("class","form-control").attr("id",(d,i) => "etiqueta_" + i).attr("value",d=> d.replaceAll("_"," "));
    left.append("label").attr("for",(d,i) => "descripcion_" + i).attr("class","form-label  mt-3").html("Descripción:");
    left.append("textarea").attr("class","form-control").attr("id",(d,i) => "descripcion_" + i).attr("rows","3");
    filas.append("hr");

    const subida = mesa.append("div").attr("class","row mt-3");
    const subot = subida.append("button").attr("type","button").attr("class","btn btn-primary").html("Subir diccionario a PNDA");

    subot.on("click", async (e,d) => {
        const filas = d3.selectAll(".fila");

        let fields = [];

        filas.each(function (col, fila) {
          let elemento = {info:{}};
          elemento["id"] = col;
          elemento["type"] = d3.select("#tipo_" + fila).property("value");
          elemento["info"]["label"] = d3.select("#etiqueta_" + fila).property("value");
          elemento["info"]["notes"] = d3.select("#descripcion_" + fila).property("value");
          elemento["info"]["type_override"] = d3.select("#tipo_" + fila).property("value");
          fields.push(elemento);
        });

        let salida = {resource_id:datos.recurso.id,force:true, fields:fields};

        const api_key = d3.select("#apikey").property("value");
        const respuesta = await invoke("enviar_datos",{apikey:api_key, datos:salida});
        console.log(respuesta);
    })

}

const appWindow = getCurrentWindow();

appWindow.onDragDropEvent(async (event) => {

    if (!document.querySelector("#dropZone")) {
        return;
    }

    await procesarDrop(event);
})

async function procesarDrop(event) {
      const dropZone = d3.select("#dropZone");

      if (dropZone.empty()) {
          return;
      }

      if (event.payload.type === 'hover') {
        dropZone.classed("dragover", true);
      } else if (event.payload.type === 'drop') {
        dropZone.classed("dragover", false);

      if (window.procesando) {
        showToast("Ya se está procesando un archivo.","danger");
        return;
      }

      window.procesando = true;

      try {

        dropZone
          .style("pointer-events", "none")
          .classed("disabled", true);

          const rutaAbsoluta = event.payload.paths[0];

          if (!rutaAbsoluta.toLowerCase().endsWith('.csv')) {
            utils.showToast("El archivo debe tener formato CSV.", "danger");
            return;
          };

          const diccionario = await invoke("leer_dicc", { ruta: rutaAbsoluta });
          apoyo(diccionario);
        
          const nombre = rutaAbsoluta.replace(/^.*[\\/]/, "");
          d3.select("#dropZone p").html(`Archivo actual: <strong>${nombre}</strong>`);

      } catch (error) {
          showToast(`No se pudo procesar el archivo. Motivo: ${error}`,"danger");
      } finally {

        dropZone
        .style("pointer-events", null)
        .classed("disabled", false);

        window.procesando = false;
      }

      } else {
        dropZone.classed("dragover", false);
      }
}

function apoyo(diccionario) {
  console.log(diccionario);

  const cont0 = d3.select("#copiar_todo");
  cont0.append("p").html("Copia y edita la información para cada columna. Si el archivo de apoyo tiene el mismo orden que la base también puedes:")
  const bcpall = cont0.append("button").attr("class","btn btn-outline-secondary btn-sm").attr("data-index",(d,i) => i).html("Copiar todo");

  const right = d3.selectAll(".right");

  right.append("label").attr("for",(d,i) => "columna_apoyo_" + i).attr("class","form-label  mt-3").html("Columna en archivo de apoyo:");
    const selecto = right.append("select").attr("class","form-select mb-3").attr("id",(d,i) => "columna_apoyo_" + i).attr("data-index", (d,i) => i);
    selecto.each(function(d,i) {

      d3.select(this)
        .selectAll("option")
        .data(diccionario)
        .join("option")
        .attr("value",(p,j) => j)
        .property("selected",(p,j) => j == i)
        .html(p => p.columna)
    });

  const dcol = right.append("div").attr("id",(d,i) => "dcol_" + i);
  dcol.append("p").html((d,i) => "Etiqueta: <strong>" + diccionario[i]["etiqueta"]+ "</strong>");
  dcol.append("p").html((d,i) => "Descripción: <strong>" + diccionario[i]["descripcion"]+ "</strong>");

  const boton = right.append("button").attr("class","btn btn-outline-secondary btn-sm").attr("data-index",(d,i) => i).html("Copiar");
  boton.on("click",(e,d) => {

    const fila = e.currentTarget.dataset.index;
    const indice = d3.select("#columna_apoyo_" + fila).property("value");

    const dato = diccionario[indice];

    d3.select("#etiqueta_" + fila)
        .property("value", dato.etiqueta);

    d3.select("#descripcion_" + fila)
        .property("value", dato.descripcion);

  })

  selecto.on("change",(e,d) => {
    const fila = e.currentTarget.dataset.index;
    const indice = e.currentTarget.value;

    const dcol = d3.select("#dcol_" + fila);
    dcol.selectAll("p").remove("*");
    dcol.append("p").html((d,i) => "Etiqueta: <strong>" + diccionario[indice]["etiqueta"]+ "</strong>");
    dcol.append("p").html((d,i) => "Descripción: <strong>" + diccionario[indice]["descripcion"]+ "</strong>");
  });

  bcpall.on("click",(e,d) => {
    d3.selectAll(".right select").each(function(_,fila) {
      const indice = this.value;
      const dato = diccionario[indice];

      d3.select("#etiqueta_" + fila)
            .property("value", dato.etiqueta);

      d3.select("#descripcion_" + fila)
          .property("value", dato.descripcion);
    })
  })
}

function mostrarGrid(selector,reporte) {
  if (window.appState.grid) {
        try {
            window.appState.grid.destroy();
        } catch (e) {
          console.warn("Error al intentar destruir el grid anterior:", e);
        }
        window.appState.grid = null;
    }

    const block = d3.select(selector);
    block.selectAll("*").remove();

    block.append("h3").html("Vista de los datos");

    const label = block.append("div").attr("class","label").append("p").html("Tipo de columna: ");
    label.selectAll(".laba").data(["Texto","Numero","Fecha"]).join("span").attr("class",d => d).html(d => d);

    block
        .append("div")
        .attr("id", "myGrid")
        .attr("class", "ag-theme-quartz")
        .style("height", "500px")
        .style("width", "100%");

    window.appState.grid = conectarGridInfinito(reporte.columnas,reporte.total_filas,reporte.esquema);
}

export function conectarGridInfinito(columnas, totalFilas, esquema) {
    const gridDiv = document.querySelector("#myGrid");
    gridDiv.innerHTML = "";

    const columnDefs = columnas.map(col => ({
        headerName: col,
        field: col,
        suppressMovable: true,
        headerClass: esquema[col],
        valueGetter: (params) => params.data?. [col] ?? '',
    }));

    const datasource = {
        getRows: async (params) => {
            try {
                const size = params.endRow - params.startRow;
                const filas = await invoke("obtener_bloque", {startRow: params.startRow, pageSize: size});

                params.successCallback(filas, totalFilas);
            } catch (error) {
                console.error("Error cargando bloque desde back:", error);
                params.failCallback();
            }
        }
    };

    const gridOptions = {
        columnDefs: columnDefs,
        rowModelType: 'infinite',
        cacheBlockSize: 100,
        maxBlocksInCache: 10,
        infiniteInitialRowCount: 1,

        defaultColDef: {
            flex: 1,
            minWidth: 150,
            resizable: true,
            sortable: false
        }
    };

    let grid = createGrid(gridDiv, gridOptions);
    grid.setGridOption('datasource', datasource);

    return grid;
}

export function showToast(message, type = "danger") {
  // type: "success", "danger", "warning", "info"

  const container = document.getElementById("toast-container");

  const toastEl = document.createElement("div");
  toastEl.className = `toast align-items-center text-bg-${type} border-0`;
  toastEl.role = "alert";
  toastEl.innerHTML = `
    <div class="d-flex">
      <div class="toast-body">${message}</div>
      <button type="button" class="btn-close btn-close-white me-2 m-auto"
              data-bs-dismiss="toast" aria-label="Close"></button>
    </div>
  `;

  container.appendChild(toastEl);

  const toast = new Toast(toastEl, { delay: 4000 });
  toast.show();

  toastEl.addEventListener("hidden.bs.toast", () => toastEl.remove());
}