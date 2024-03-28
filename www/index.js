import * as wasm_module from "rsdepth";
import {
  combineDicts,
  fetchDepthAsync,
  fetchHistOI,
  fetchOI,
  initialKlineFetch,
  fetchHistTrades,
  fetchTickerInfo,
  tickersOIfetch,
} from "./connectorUtils.js";

console[window.crossOriginIsolated ? "log" : "error"](
  "Cross-origin isolation is " +
    (window.crossOriginIsolated ? "enabled" : "not enabled")
);
typeof SharedArrayBuffer !== "undefined"
  ? console.log("TriHard enabled (SharedArrayBuffer is available)")
  : console.error("SharedArrayBuffer is not available in this environment");

const buttons = ["btn1", "btn2", "btn3", "btn4"];
const menuIds = ["tickers-menu", "menu2", "menu3", "settings-menu"];
const functions = [showTickers, toggleAutoScale, buttonTest, showSettings];

for (let i = 0; i < buttons.length; i++) {
  const button = document.getElementById(buttons[i]);
  button.addEventListener("click", functions[i]);
  button.addEventListener("click", function () {
    updateButtonState(buttons[i], menuIds[i]);
  });
}

const tickersMenu = document.getElementById("tickers-menu");
const settingsMenu = document.getElementById("settings-menu");

let input = document.getElementById("ticker-search");
let searchTerm;
input.addEventListener("keyup", function () {
  searchTerm = this.value.toLowerCase();
  let rows = document.querySelectorAll("#ticker-table tbody tr");

  for (let row of rows) {
    let symbol = row.cells[0].textContent.toLowerCase();

    if (symbol.includes(searchTerm)) {
      row.style.display = "";
    } else {
      row.style.display = "none";
    }
  }
});

function getCurrentTime() {
  const now = new Date();
  const hours = now.getHours().toString().padStart(2, "0");
  const minutes = now.getMinutes().toString().padStart(2, "0");
  const seconds = now.getSeconds().toString().padStart(2, "0");
  return hours + ":" + minutes + ":" + seconds;
}
function updateLastUpdatedInfo() {
  const tickersUpdateInfo = document.getElementById("tickers-update-info");
  tickersUpdateInfo.textContent = "Last updated at " + getCurrentTime();
}

const tickersUpdateBtn = document.getElementById("tickers-update-btn");
tickersUpdateBtn.addEventListener("click", function () {
  updateTable();
});

function updateTable() {
  tickersUpdateBtn.className = "loading-animation";
  tickersUpdateBtn.disabled = true;
  combineDicts().then((data) => {
    generateTable(data);

    let currentTime = Date.now();
    let startTime = currentTime - 25 * 60 * 60 * 1000;
    let endTime = startTime + 60 * 60 * 1000;

    tickersOIfetch(Object.keys(data), startTime, endTime).then(
      (hist_OI_data) => {
        Object.keys(hist_OI_data).forEach((symbol) => {
          if (data.hasOwnProperty(symbol)) {
            data[symbol] = {
              ...data[symbol],
              ...hist_OI_data[symbol],
            };
          }
        });
        generateTable(data);
        tickersUpdateBtn.disabled = false;
        tickersUpdateBtn.className = "";
      }
    );
    updateLastUpdatedInfo();
  });
}
updateTable();

const autoScaleBtnPath = document.querySelector("#btn2 > svg > path");
let isAutoScale = true;
function updateUI() {
  isAutoScale = manager.get_autoscale();
  console.log("autoscale:", isAutoScale);
  if (!isAutoScale) {
    autoScaleBtnPath.setAttribute(
      "d",
      "M144 144c0-44.2 35.8-80 80-80c31.9 0 59.4 18.6 72.3 45.7c7.6 16 26.7 22.8 42.6 15.2s22.8-26.7 15.2-42.6C331 33.7 281.5 0 224 0C144.5 0 80 64.5 80 144v48H64c-35.3 0-64 28.7-64 64V448c0 35.3 28.7 64 64 64H384c35.3 0 64-28.7 64-64V256c0-35.3-28.7-64-64-64H144V144z"
    );
  } else if (isAutoScale) {
    autoScaleBtnPath.setAttribute(
      "d",
      "M144 144v48H304V144c0-44.2-35.8-80-80-80s-80 35.8-80 80zM80 192V144C80 64.5 144.5 0 224 0s144 64.5 144 144v48h16c35.3 0 64 28.7 64 64V448c0 35.3-28.7 64-64 64H64c-35.3 0-64-28.7-64-64V256c0-35.3 28.7-64 64-64H80z"
    );
  }
}

function toggleAutoScale() {
  manager.toggle_autoscale();
  updateUI();
}
function buttonTest() {
  console.log("button test");
}
function showTickers() {
  input.value = "";
  searchTerm = "";
  let rows = document.querySelectorAll("#ticker-table tbody tr");

  for (let row of rows) {
    row.style.display = "";
  }
  tickersMenu.style.display =
    tickersMenu.style.display === "none" ? "block" : "none";
  updateButtonState("btn1", "tickers-menu");

  if (tickersMenu.style.display === "block") {
    document.addEventListener("click", closeMenu);
  } else {
    document.removeEventListener("click", closeMenu);
  }
}
function showSettings() {
  settingsMenu.style.display =
    settingsMenu.style.display === "none" ? "block" : "none";
  updateButtonState("btn4", "settings-menu");

  if (settingsMenu.style.display === "block") {
    document.addEventListener("click", closeMenu);
  } else {
    document.removeEventListener("click", closeMenu);
  }
}
function closeMenu(e) {
  const btn1 = document.querySelector("#btn1");
  const btn4 = document.querySelector("#btn4");

  if (!settingsMenu.contains(e.target) && !btn4.contains(e.target)) {
    settingsMenu.style.display = "none";
    updateButtonState("btn4", "settings-menu");
  }
  if (!tickersMenu.contains(e.target) && !btn1.contains(e.target)) {
    tickersMenu.style.display = "none";
    updateButtonState("btn1", "tickers-menu");
  }

  if (
    settingsMenu.style.display === "none" &&
    tickersMenu.style.display === "none"
  ) {
    document.removeEventListener("click", closeMenu);
  }
}

function updateButtonState(buttonId, menuId) {
  const menu = document.getElementById(menuId);
  const button = document.getElementById(buttonId);

  if (buttonId === "btn1" || buttonId === "btn4") {
    if (menu.style.display === "block") {
      button.classList.add("active");
    } else {
      button.classList.remove("active");
    }
  }
}

function formatLargeNumber(num) {
  if (num >= 1.0e9) {
    return (num / 1.0e9).toFixed(2) + "b";
  } else if (num >= 1.0e6) {
    return (num / 1.0e6).toFixed(2) + "m";
  } else if (num >= 1.0e3) {
    return (num / 1.0e3).toFixed(2) + "k";
  } else {
    return num;
  }
}
function formatNumber(value, type, price) {
  let displayValue;

  if (type === "mark_price") {
    if (value > 10) {
      displayValue = Math.round(value * 100) / 100;
    } else {
      displayValue = Math.round(value * 10000) / 10000;
    }
  } else if (type === "volume") {
    displayValue = formatLargeNumber(value);
    displayValue = "$" + displayValue;
  } else if (type === "open_interest") {
    displayValue = formatLargeNumber(value * price);
    displayValue = "$" + displayValue;
  }
  return displayValue;
}
function generateTable(data) {
  let tableBody = document.querySelector("#tickers-menu table tbody");
  tableBody.innerHTML = "";

  let entries = Object.entries(data);
  entries.sort(([, a], [, b]) => b.volume - a.volume);

  for (let i = 0; i < entries.length; i++) {
    let [symbol, symbolData] = entries[i];
    let row;

    if (i < tableBody.rows.length) {
      row = tableBody.rows[i];
    } else {
      row = tableBody.insertRow();
      row.insertCell(); // symbol
      row.insertCell(); // mark_price
      row.insertCell(); // change
      row.insertCell(); // funding
      row.insertCell(); // OI
      row.insertCell(); // OI change
      row.insertCell(); // volume
    }
    row.classList.add("table-row");

    row.cells[0].textContent = symbol;
    row.cells[1].textContent = formatNumber(
      symbolData.mark_price,
      "mark_price",
      symbolData.mark_price
    );
    row.cells[2].textContent =
      (Math.round(symbolData.change * 100) / 100).toFixed(2) + "%";
    row.cells[3].textContent = symbolData.funding_rate + "%";
    row.cells[4].textContent =
      data.hasOwnProperty(symbol) &&
      data[symbol].hasOwnProperty("open_interest")
        ? formatNumber(
            data[symbol].open_interest,
            "open_interest",
            data[symbol].mark_price
          )
        : "...";
    row.cells[5].textContent =
      data.hasOwnProperty(symbol) &&
      data[symbol].hasOwnProperty("OI_24hrChange")
        ? data[symbol].OI_24hrChange + "%"
        : "...";
    row.cells[6].textContent = formatNumber(
      symbolData.volume,
      "volume",
      symbolData.mark_price
    );

    const chng_color_a = Math.min(Math.abs(symbolData.change / 100), 1);
    const fndng_color_a = Math.max(Math.abs(symbolData.funding_rate * 50), 0.2);

    if (symbolData.change < 0) {
      row.style.backgroundColor =
        "rgba(192, 80, 78, " + chng_color_a * 1.5 + ")";
    } else {
      row.style.backgroundColor = "rgba(81, 205, 160, " + chng_color_a + ")";
    }
    if (symbolData.funding_rate > 0) {
      row.cells[3].style.color =
        "rgba(212, 80, 78, " + fndng_color_a * 1.5 + ")";
    } else {
      row.cells[3].style.color =
        "rgba(81, 246, 160, " + fndng_color_a * 1.5 + ")";
    }
    row.addEventListener("click", function () {
      canvasStarter(symbol, symbolData.mark_price);
    });
  }
}

function canvasStarter(symbol, price) {
  changeSymbol(symbol.toLowerCase());
}

function debounce(func, wait) {
  let timeout;
  return function executedFunction(...args) {
    const later = () => {
      clearTimeout(timeout);
      func(...args);
    };
    clearTimeout(timeout);
    timeout = setTimeout(later, wait);
  };
}

function adjustDPI(canvas) {
  let dpi = window.devicePixelRatio;
  let style_height = +getComputedStyle(canvas)
    .getPropertyValue("height")
    .slice(0, -2);
  let style_width = +getComputedStyle(canvas)
    .getPropertyValue("width")
    .slice(0, -2);
  canvas.setAttribute("height", style_height * dpi);
  canvas.setAttribute("width", style_width * dpi);
  return { width: style_width * dpi, height: style_height * dpi };
}

let canvasIds = [
  "#canvas-main",
  "#canvas-depth",
  "#canvas-indi-2",
  "#canvas-bubble",
  "#canvas-indi-1",
];
let canvases = canvasIds.map((id) => {
  let canvas = document.querySelector(id);
  canvas.getContext("2d", { alpha: false });
  return canvas;
});
let dimensions = canvases.map(adjustDPI);

window.addEventListener(
  "resize",
  debounce(() => {
    console.log("resizing...");
    dimensions = canvases.map(adjustDPI);
    manager.resize(
      dimensions.map((d) => d.width),
      dimensions.map((d) => d.height)
    );
  }, 300)
);

let manager = wasm_module.CanvasManager.new(...canvases);
let depthIntervalId, oiIntervalId;
let currentSymbol = "btcusdt";
changeSymbol(currentSymbol);

function renderLoop() {
  manager.render_start();
  setTimeout(() => {
    requestAnimationFrame(renderLoop);
  }, 1000 / 30);
}
requestAnimationFrame(renderLoop);

function changeSymbol(newSymbol) {
  depthIntervalId ? clearInterval(depthIntervalId) : null;
  oiIntervalId ? clearInterval(oiIntervalId) : null;

  currentSymbol = newSymbol;

  fetchTickerInfo(currentSymbol).then(([tickSize, minQty]) => {
    manager.set_symbol_info(tickSize, minQty, tickSizeBtn.value);
  });

  manager.initialize_ws(currentSymbol);

  fetchDepthAsync(currentSymbol).then((depth) => {
    manager.gather_depth(depth);
  });
  initialKlineFetch(currentSymbol).then((klines) => {
    manager.gather_klines(klines);
    const keys = manager.get_kline_ohlcv_keys();
    getHistTrades(currentSymbol, keys, manager);
  });
  fetchHistOI(currentSymbol).then((histOI) => {
    manager.gather_hist_oi(histOI);
  });

  scheduleFetchOI();
  scheduleFetchDepth();

  document.querySelector("#tickerInfo-name").textContent =
    currentSymbol.toUpperCase();
  if (tickersMenu.style.display === "block") {
    showTickers();
  }
}
function scheduleFetchDepth() {
  depthIntervalId = setInterval(() => {
    fetchDepthAsync(currentSymbol).then((depth) => {
      manager.gather_depth(depth);
    });
  }, 12000);
}
function scheduleFetchOI() {
  const now = new Date();
  const delay = (60 - now.getSeconds() - 1) * 1000 - now.getMilliseconds();

  setTimeout(() => {
    fetchOI(currentSymbol).then((oi) => {
      manager.gather_oi(oi);
    });
    if (oiIntervalId) {
      clearInterval(oiIntervalId);
    }
    oiIntervalId = setInterval(() => {
      fetchOI(currentSymbol).then((oi) => {
        manager.gather_oi(oi);
      });
    }, 60000);
  }, delay);
}

const tickSizeBtn = document.querySelector("#ticksize-select");
tickSizeBtn.addEventListener("change", function () {
  manager.set_tick_size(tickSizeBtn.value);
});

async function getHistTrades(symbol, dp, manager) {
  // get current kline first
  let startTime = Number(dp[dp.length - 1]) + 60000;
  const endTime = Date.now();
  let trades = [];
  let lastTradeTime = 0;
  console.log("getting current trades...");
  do {
    try {
      const fetchedTrades = await fetchHistTrades(
        symbol,
        startTime,
        endTime,
        1000
      );
      trades = trades.concat(fetchedTrades);
      lastTradeTime = fetchedTrades[fetchedTrades.length - 1].x;
      startTime = lastTradeTime + 1;
      console.log("fetched", fetchedTrades.length, "trades");
    } catch (error) {
      console.log(error, startTime, null);
      break;
    }
  } while (lastTradeTime < endTime);
  manager.gather_hist_trades(
    JSON.stringify(trades),
    (endTime - 59999).toString()
  );

  // get historical klines after
  for (let i = dp.length - 1; i >= 0; i--) {
    let startTime = Number(dp[i]);
    const endTime = startTime + 59999;
    let trades = [];
    let lastTradeTime = 0;
    console.log(
      "getting historical trades:",
      i + 1,
      "of",
      dp.length,
      "klines..."
    );
    while (true) {
      if (symbol != currentSymbol) {
        console.log("stopped fetching historical trades for", symbol);
        return;
      }
      try {
        const fetchedTrades = await fetchHistTrades(
          symbol,
          startTime,
          endTime,
          1000
        );
        trades = trades.concat(fetchedTrades);
        if (fetchedTrades.length > 0) {
          lastTradeTime = fetchedTrades[fetchedTrades.length - 1].time;
          startTime = lastTradeTime + 1;
        }
        if (fetchedTrades.length < 1000) {
          break;
        }
        await new Promise((resolve) => setTimeout(resolve, 400));
      } catch (error) {
        console.log(error, startTime, endTime);
        break;
      }
    }
    manager.gather_hist_trades(
      JSON.stringify(trades),
      (endTime - 59999).toString()
    );
  }
}

// Canvas event listeners //
let canvasMain = document.querySelector("#canvas-main");
let canvasIndi1 = document.querySelector("#canvas-indi-1");
let canvasIndi2 = document.querySelector("#canvas-indi-2");
let canvasDepth = document.querySelector("#canvas-depth");

// Panning
let isDragging = false;
let initialXY = [0, 0];
canvasMain.addEventListener("mousedown", function (event) {
  isDragging = true;
  initialXY = { x: event.clientX, y: event.clientY };
});
canvasMain.addEventListener("mousemove", function (event) {
  if (isDragging) {
    manager.pan_xy(event.clientX - initialXY.x, event.clientY - initialXY.y);
    initialXY = { x: event.clientX, y: event.clientY };
  }
});
canvasMain.addEventListener("mouseup", function (event) {
  isDragging = false;
});
canvasMain.addEventListener("mouseleave", function (event) {
  isDragging = false;
});

// Zoom X
canvasIndi1.addEventListener("wheel", function (event) {
  event.preventDefault();
  manager.zoom_x(-event.deltaY);
});
canvasIndi2.addEventListener("wheel", function (event) {
  event.preventDefault();
  manager.zoom_x(-event.deltaY);
});

// Zoom Y
canvasDepth.addEventListener("wheel", function (event) {
  if (!isAutoScale) {
    event.preventDefault();
    manager.zoom_y(event.deltaY);
  } else if (isAutoScale) {
    toggleAutoScale();
  }
});

// Zoom XY
canvasMain.addEventListener("wheel", function (event) {
  event.preventDefault();
  manager.zoom_x(-event.deltaY);
  if (!isAutoScale) {
    manager.zoom_y(event.deltaY);
  } else if (isAutoScale) {
    toggleAutoScale();
  }
});
