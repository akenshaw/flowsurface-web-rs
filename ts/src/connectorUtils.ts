interface PremiumData {
    funding_rate: number;
    mark_price: number;
}

interface TurnoverData {
    change: number;
    volume: number;
}

export interface CombinedData
    extends PremiumData,
        TurnoverData,
        OpenInterestData {}

interface OpenInterestData {
    open_interest: number;
    OI_24hrChange: number;
}

interface BinanceResponse {
    symbol: string;
    lastFundingRate: string;
    markPrice: string;
    openInterest: string;
    quoteVolume: string;
    priceChangePercent: string;
}

interface HistoricalTrade {
    price: number;
    quantity: number;
    time: number;
    is_buyer_maker: boolean;
}

interface SymbolFilter {
    tickSize?: string;
    minQty?: string;
    filterType: string;
    [key: string]: unknown;
}

interface SymbolInfo {
    symbol: string;
    filters: SymbolFilter[];
    [key: string]: unknown;
}

interface ExchangeInfo {
    symbols: SymbolInfo[];
    [key: string]: unknown;
}

interface ExchangeInfo {
    symbols: SymbolInfo[];
}

export async function combineDicts(): Promise<Record<string, CombinedData>> {
    const combined_dict: Record<string, CombinedData> = {};

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
                ...{ open_interest: NaN, OI_24hrChange: NaN },
            };
        }
    });

    return combined_dict;
}

export async function tickersOIfetch(
    symbols: string[],
    startTime: number,
    endTime: number
): Promise<Record<string, OpenInterestData>> {
    const hist_OI_promises = symbols.map((symbol) =>
        fetch_hist_OI(symbol, startTime, endTime)
    );
    const hist_OI_data_arr = await Promise.all(hist_OI_promises);

    const hist_OI_dict: Record<string, OpenInterestData> = {};
    symbols.forEach((symbol, i) => {
        hist_OI_dict[symbol] = hist_OI_data_arr[i];
    });

    return hist_OI_dict;
}

async function fetch_hist_OI(
    symbol: string,
    startTime: number,
    endTime: number
): Promise<OpenInterestData> {
    let current_OI: number;
    try {
        current_OI = await fetch_current_OI(symbol);
        const response = await fetch(
            `https://fapi.binance.com/futures/data/openInterestHist?symbol=${symbol}&period=30m&limit=1&startTime=${startTime}&endTime=${endTime}`
        );
        const data = await response.json();

        const OI_24hrAgo = Math.round(Number(data[0].sumOpenInterest));
        const OI_24hrChange =
            Math.round(((current_OI - OI_24hrAgo) / OI_24hrAgo) * 10000) / 100;

        return { open_interest: current_OI, OI_24hrChange };
    } catch (error) {
        console.error(error, symbol);
        return { open_interest: current_OI!, OI_24hrChange: NaN };
    }
}

async function fetch_current_OI(symbol: string): Promise<number> {
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

async function fetchPremiumIndexes(): Promise<Record<string, PremiumData>> {
    let fr_dict: Record<string, PremiumData> = {};

    const response = await fetch(
        `https://fapi.binance.com/fapi/v1/premiumIndex`
    );
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

async function fetch24hrMetrics(): Promise<Record<string, TurnoverData>> {
    let turnovers_dict: Record<string, TurnoverData> = {};

    const response = await fetch(
        `https://fapi.binance.com/fapi/v1/ticker/24hr`
    );
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
export async function fetchOI(symbol: string) {
    try {
        let response = await fetch(
            `https://fapi.binance.com/fapi/v1/openInterest?symbol=${symbol}`
        );
        return await response.text();
    } catch (error) {
        console.error(error);
    }
}

export async function fetchHistOI(symbol: string) {
    try {
        let response = await fetch(
            `https://fapi.binance.com/futures/data/openInterestHist?symbol=${symbol}&period=5m&limit=12`
        );
        return await response.text();
    } catch (error) {
        console.error(error);
    }
}

export async function fetchDepthAsync(symbol: string) {
    try {
        let response = await fetch(
            `https://fapi.binance.com/fapi/v1/depth?symbol=${symbol}&limit=1000`
        );
        return await response.text();
    } catch (error) {
        console.error(error);
    }
}

export async function initialKlineFetch(symbol: string) {
    try {
        let response = await fetch(
            `https://fapi.binance.com/fapi/v1/klines?symbol=${symbol}&interval=1m&limit=60`
        );
        return await response.text();
    } catch (error) {
        console.error(error);
    }
}

interface BinanceAggTrade {
    p: string; // Price
    q: string; // Quantity
    T: number; // Timestamp
    m: boolean; // Is buyer maker
    [key: string]: unknown;
}

export async function fetchHistTrades(
    symbol: string,
    startTime: number,
    endTime: number,
    limit: number,
    retryCount: number = 0
): Promise<HistoricalTrade[]> {
    const url = `https://fapi.binance.com/fapi/v1/aggTrades?symbol=${symbol}${
        startTime ? "&startTime=" + startTime : ""
    }${endTime ? "&endTime=" + endTime : ""}${limit ? "&limit=" + limit : ""}`;

    try {
        const response = await fetch(url);

        if (response.status === 429) {
            const waitTime = Math.pow(2, retryCount) * 1000;
            console.log(
                `Rate limit exceeded, pausing for ${waitTime / 1000} seconds...`
            );
            await new Promise((resolve) => setTimeout(resolve, waitTime));
            return fetchHistTrades(
                symbol,
                startTime,
                endTime,
                limit,
                retryCount + 1
            );
        }

        const data = (await response.json()) as BinanceAggTrade[];
        const trades = data.map((trade: BinanceAggTrade): HistoricalTrade => {
            return {
                price: parseFloat(trade.p),
                quantity: parseFloat(trade.q),
                time: trade.T,
                is_buyer_maker: trade.m,
            };
        });

        return trades;
    } catch (error) {
        console.log(error, url);
        return [];
    }
}

export async function fetchTickerInfo(
    symbol: string
): Promise<[number, number] | null> {
    const response = await fetch(
        `https://fapi.binance.com/fapi/v1/exchangeInfo`
    );
    const data = (await response.json()) as ExchangeInfo;

    const symbol_info = data.symbols.find(
        (x) => x.symbol === symbol.toUpperCase()
    );

    if (symbol_info) {
        const tickSizeFilter = symbol_info.filters.find(
            (f) => f.filterType === "PRICE_FILTER"
        );
        const lotSizeFilter = symbol_info.filters.find(
            (f) => f.filterType === "LOT_SIZE"
        );

        return [
            parseFloat(tickSizeFilter?.tickSize as string),
            parseFloat(lotSizeFilter?.minQty as string),
        ];
    }

    return null;
}
