// tickers table fetch
export async function combineDicts() {
  let combined_dict = {};

  const [fr_dict, turnovers_dict] = await Promise.all([
    fetchPremiumIndexes(),
    fetch24hrMetrics(),
  ]);

  const symbols = Object.keys(fr_dict).filter(
    (symbol) =>
      ![
        "BTCSTUSDT",
        "CVCUSDT",
        "DOGEUSDC",
        "COCOSUSDT",
        "BNBBTC",
        "ETHBTC",
        "RAYUSDT",
        "HNTUSDT",
        "SCUSDT",
        "WIFUSDT",
        "BTSUSDT",
        "TOMOUSDT",
        "FTTUSDT",
        "SRMUSDT",
      ].includes(symbol) && !symbol.includes("_")
  );
  symbols.forEach((symbol) => {
    if (turnovers_dict.hasOwnProperty(symbol)) {
      combined_dict[symbol] = {
        ...fr_dict[symbol],
        ...turnovers_dict[symbol],
      };
    }
  });
  return combined_dict;
}
export async function tickersOIfetch(symbols, startTime, endTime) {
  const hist_OI_promises = symbols.map((symbol) =>
    fetch_hist_OI(symbol, startTime, endTime)
  );
  const hist_OI_data_arr = await Promise.all(hist_OI_promises);

  let hist_OI_dict = {};
  symbols.forEach((symbol, i) => {
    hist_OI_dict[symbol] = {
      ...hist_OI_data_arr[i],
    };
  });

  return hist_OI_dict;
}
async function fetch_hist_OI(symbol, startTime, endTime) {
  let current_OI;
  try {
    current_OI = await fetch_current_OI(symbol);
    const response = await fetch(
      `https://fapi.binance.com/futures/data/openInterestHist?symbol=${symbol}&period=30m&limit=1&startTime=${startTime}&endTime=${endTime}`
    );
    const data = await response.json();

    const OI_24hrAgo = Math.round(data[0].sumOpenInterest);
    const OI_24hrChange =
      Math.round(((current_OI - OI_24hrAgo) / OI_24hrAgo) * 10000) / 100;

    return { open_interest: current_OI, OI_24hrChange: OI_24hrChange };
  } catch (error) {
    console.error(error, symbol);
    return { open_interest: current_OI, OI_24hrChange: NaN };
  }
}
async function fetch_current_OI(symbol) {
  try {
    const response = await fetch(
      `https://fapi.binance.com/fapi/v1/openInterest?symbol=${symbol}`
    );
    const data = await response.json();
    return Number(data["openInterest"]);
  } catch (error) {
    console.error(error, symbol);
    return NaN;
  }
}
async function fetchPremiumIndexes() {
  let fr_dict = {};

  const response = await fetch(`https://fapi.binance.com/fapi/v1/premiumIndex`);
  const data = await response.json();

  for (let i of data) {
    let symbol = i["symbol"];
    let funding_rate = parseFloat(i["lastFundingRate"]) * 100;
    let mark_price = parseFloat(i["markPrice"]);

    fr_dict[symbol] = {
      funding_rate: parseFloat(funding_rate.toFixed(3)),
      mark_price: Math.round(mark_price * 10000) / 10000,
    };
  }
  return fr_dict;
}
async function fetch24hrMetrics() {
  let turnovers_dict = {};

  const response = await fetch(`https://fapi.binance.com/fapi/v1/ticker/24hr`);
  const data = await response.json();
  for (let i of data) {
    let symbol = i["symbol"];
    let volume = i["quoteVolume"];
    let changeP = i["priceChangePercent"];

    turnovers_dict[symbol] = {
      change: Number(changeP),
      volume: Math.round(volume),
    };
  }
  return turnovers_dict;
}

// canvas fetch
export async function fetchOI(symbol) {
  try {
    let response = await fetch(
      `https://fapi.binance.com/fapi/v1/openInterest?symbol=${symbol}`
    );
    return await response.text();
  } catch (error) {
    console.error(error);
  }
}
export async function fetchHistOI(symbol) {
  try {
    let response = await fetch(
      `https://fapi.binance.com/futures/data/openInterestHist?symbol=${symbol}&period=5m&limit=12`
    );
    return await response.text();
  } catch (error) {
    console.error(error);
  }
}
export async function fetchDepthAsync(symbol) {
  try {
    let response = await fetch(
      `https://fapi.binance.com/fapi/v1/depth?symbol=${symbol}&limit=1000`
    );
    return await response.text();
  } catch (error) {
    console.error(error);
  }
}
export async function initialKlineFetch(symbol) {
  try {
    let response = await fetch(
      `https://fapi.binance.com/fapi/v1/klines?symbol=${symbol}&interval=1m&limit=60`
    );
    return await response.text();
  } catch (error) {
    console.error(error);
  }
}
export async function fetchHistTrades(
  symbol,
  startTime,
  endTime,
  limit,
  retryCount = 0
) {
  try {
    const url = `https://fapi.binance.com/fapi/v1/aggTrades?symbol=${symbol}${
      startTime ? "&startTime=" + startTime : ""
    }${endTime ? "&endTime=" + endTime : ""}${limit ? "&limit=" + limit : ""}`;
    const response = await fetch(url);

    if (response.status === 429) {
      const waitTime = Math.pow(2, retryCount) * 1000;
      console.log(
        `Rate limit exceeded, pausing for ${waitTime / 1000} seconds...`
      );
      await new Promise((resolve) => setTimeout(resolve, waitTime));
      return fetchHistTrades(symbol, startTime, endTime, limit, retryCount + 1);
    }

    const data = await response.json();
    const trades = data.map((trade) => {
      return {
        price: parseFloat(trade.p),
        quantity: parseFloat(trade.q),
        time: trade.T,
        is_buyer_maker: trade.m,
      };
    });

    //console.log(`Fetched ${trades.length} trades.`);
    return trades;
  } catch (error) {
    console.log(error, url);
    return NaN;
  }
}
export async function fetchTickerInfo(symbol) {
  const response = await fetch(`https://fapi.binance.com/fapi/v1/exchangeInfo`);
  const data = await response.json();

  let symbol_info = data["symbols"].find(
    (x) => x.symbol === symbol.toUpperCase()
  );
  if (symbol_info) {
    return [
      symbol_info["filters"][0]["tickSize"],
      symbol_info["filters"][2]["minQty"],
    ];
  } else {
    return null;
  }
}
