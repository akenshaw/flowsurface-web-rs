mod utils;

use std::collections::{HashMap, BTreeMap};
use std::sync::{Arc, RwLock};
use std::cell::RefCell;
use std::rc::Rc;
use serde::Deserialize;

use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, MessageEvent, WebSocket};
use serde_json::Value;

extern crate js_sys;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}
#[derive(Copy, Clone, Deserialize, Debug)]
pub struct Trade {
    price: f64,
    quantity: f64,
    time: u64,
    is_buyer_maker: bool,
}
#[derive(Clone, Debug)]
pub struct Order {
    price: f64,
    quantity: f64,
}
#[derive(Debug)]
pub struct Kline {
    open_time: u64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    buy_volume: f64,
    sell_volume: f64,
    cum_volume_delta: f64,
    close_time: u64,
}
#[derive(Debug)]
pub struct TradeGroups {
    buy_trades: Vec<(f64, f64)>,
    sell_trades: Vec<(f64, f64)>,
}
#[derive(Debug)]
pub struct GroupedTrades {
    buys: HashMap<i64, f64>,
    sells: HashMap<i64, f64>,
}

#[wasm_bindgen]
pub struct CanvasManager {
    klines_ohlcv: Arc<RwLock<BTreeMap<u64, Kline>>>,
    klines_trades: Arc<RwLock<BTreeMap<u64, TradeGroups>>>,
    orderbook_manager: OrderbookManager,
    oi_datapoints: Arc<RwLock<Vec<(u64, f64)>>>,
    canvas_main: CanvasMain,
    canvas_orderbook: CanvasOrderbook,
    canvas_indicator_volume: CanvasIndicatorVolume,
    canvas_bubble: Rc<RefCell<CanvasBubbleTrades>>,
    canvas_indi_cvd: CanvasIndiCVD,
    pan_x_offset: f64,
    x_zoom: f64,
    bucket_size: Arc<RwLock<f64>>,
    last_depth_update: Rc<RefCell<u64>>,
    websocket: Option<WebSocket>,
    tick_size: Rc<RefCell<f64>>
}
#[wasm_bindgen]
impl CanvasManager {
    pub fn new(canvas1: HtmlCanvasElement, canvas2: HtmlCanvasElement, canvas3: HtmlCanvasElement, canvas4: HtmlCanvasElement, canvas5: HtmlCanvasElement) -> Self {
        Self {
            klines_ohlcv: Arc::new(RwLock::new(BTreeMap::new())),
            klines_trades: Arc::new(RwLock::new(BTreeMap::new())),
            orderbook_manager: OrderbookManager::new(),
            oi_datapoints: Arc::new(RwLock::new(Vec::new())),
            canvas_main: CanvasMain::new(canvas1).expect("Failed to create CanvasMain"),
            canvas_orderbook: CanvasOrderbook::new(canvas2).expect("Failed to create CanvasOrderbook"),
            canvas_indicator_volume: CanvasIndicatorVolume::new(canvas3).expect("Failed to create CanvasIndicatorVolume"),
            canvas_bubble: Rc::new(RefCell::new(CanvasBubbleTrades::new(canvas4).expect("Failed to create CanvasBubbleTrades"))),
            canvas_indi_cvd: CanvasIndiCVD::new(canvas5).expect("Failed to create CanvasIndiCVD"),
            pan_x_offset: 0.0,
            x_zoom: 30.0,
            bucket_size: Arc::new(RwLock::new(5.0)),
            last_depth_update: Rc::new(RefCell::new(0)),
            websocket: None,
            tick_size: Rc::new(RefCell::new(0.1)),
        }
    }
    
    pub async fn initialize_ws(&mut self, symbol: &str) {
        self.start_websocket(symbol).await;
    }
    
    pub async fn start_websocket(&mut self, symbol: &str) {
        if let Some(ws) = &self.websocket {
            log("Closing existing websocket");
            ws.close().unwrap();
            self.clear_datasets();
            *self.tick_size.borrow_mut() = 0.1;
        }
        
        let mut current_kline_open: u64 = 0;
        let mut trades_buffer: Vec<Trade> = Vec::new();

        let klines_trades = Arc::clone(&self.klines_trades);
        let klines_ohlcv = Arc::clone(&self.klines_ohlcv);

        let bids = Arc::clone(&self.orderbook_manager.bids);
        let asks = Arc::clone(&self.orderbook_manager.asks);
        //let last_update_id = Arc::clone(&self.orderbook_manager.last_update_id);
        let last_depth_update = Rc::clone(&self.last_depth_update);

        let canvas_bubble = Rc::clone(&self.canvas_bubble);

        log(format!("Starting websocket for {}", symbol).as_str());

        let ws = WebSocket::new(&format!("wss://fstream.binance.com/stream?streams={}@aggTrade/{}@depth@100ms/{}@kline_1m", symbol, symbol, symbol)).unwrap();

        let onmessage_callback = Closure::wrap(Box::new(move |event: MessageEvent| {
            if let Ok(data) = event.data().dyn_into::<js_sys::JsString>() {
                let data_str: String = data.into();
                let v: Value = serde_json::from_str(&data_str).unwrap();

                match v["stream"].as_str() {
                    Some(stream) if stream.contains("aggTrade") => {
                        if let (Some(price), Some(quantity), Some(time), Some(is_buyer_maker)) = 
                        (v["data"]["p"].as_str(), v["data"]["q"].as_str(), v["data"]["T"].as_u64(), v["data"]["m"].as_bool()) {
                            if let (Ok(price), Ok(quantity)) = (price.parse::<f64>(), quantity.parse::<f64>()) {
                                let trade = Trade { price, quantity, time, is_buyer_maker };
                                trades_buffer.push(trade);
                            }
                        }
                    },
                    Some(stream) if stream.contains("depth") => {
                        if let Some(bids_array) = v["data"]["b"].as_array() {
                            if let Ok(mut bids_borrowed) = bids.try_write() {
                                let min_price = bids_borrowed.iter().map(|x| x.price).fold(f64::INFINITY, |a, b| a.min(b));
                                let bids_array: Vec<&Value> = bids_array.iter().filter(|x| {
                                    if let Some(price) = x[0].as_str().and_then(|s| s.parse::<f64>().ok()) {
                                        price >= min_price
                                    } else {
                                        false
                                    }
                                }).collect();
                                for bid in bids_array {
                                    let price = match bid[0].as_str().and_then(|s| s.parse::<f64>().ok()) {
                                        Some(price) => price,
                                        None => continue,
                                    };
                                    let quantity = match bid[1].as_str().and_then(|s| s.parse::<f64>().ok()) {
                                        Some(quantity) => quantity,
                                        None => continue,
                                    };
                                    if quantity == 0.0 {
                                        bids_borrowed.retain(|x| x.price != price);
                                    } else if let Some(bid) = bids_borrowed.iter_mut().find(|x| x.price == price) {
                                        bid.quantity = quantity;
                                    } else {
                                        bids_borrowed.push(Order { price, quantity });
                                    }
                                }
                            } else {
                                log(&format!("bids locked on render"));
                            }
                        }
                        if let Some(asks_array) = v["data"]["a"].as_array() {
                            if let Ok(mut asks_borrowed) = asks.try_write() {
                                let max_price = asks_borrowed.iter().map(|x| x.price).fold(f64::NEG_INFINITY, |a, b| a.max(b));
                                let asks_array: Vec<&Value> = asks_array.iter().filter(|x| {
                                    if let Some(price) = x[0].as_str().and_then(|s| s.parse::<f64>().ok()) {
                                        price <= max_price
                                    } else {
                                        false
                                    }
                                }).collect();
                                for ask in asks_array {
                                    let price = match ask[0].as_str().and_then(|s| s.parse::<f64>().ok()) {
                                        Some(price) => price,
                                        None => continue,
                                    };
                                    let quantity = match ask[1].as_str().and_then(|s| s.parse::<f64>().ok()) {
                                        Some(quantity) => quantity,
                                        None => continue,
                                    };
                                    if quantity == 0.0 {
                                        asks_borrowed.retain(|x| x.price != price);
                                    } else if let Some(ask) = asks_borrowed.iter_mut().find(|x| x.price == price) {
                                        ask.quantity = quantity;
                                    } else {
                                        asks_borrowed.push(Order { price, quantity });
                                    }
                                }
                            } else {
                                log(&format!("asks locked on render"));
                            }
                        }
                        if let Some(update_id) = v["data"]["T"].as_u64() {
                            *last_depth_update.borrow_mut() = update_id;
                        }
    
                        if current_kline_open != 0 {
                            match klines_trades.try_write() {
                                Ok(mut klines_trades) => {
                                    if let Some(update_time) = v["data"]["T"].as_u64() {
                                        let mut canvas_bubble = canvas_bubble.borrow_mut();
                                        canvas_bubble.render(&trades_buffer, update_time);
                                    }

                                    let trade_groups = klines_trades.entry(current_kline_open).or_insert(TradeGroups { buy_trades: Vec::new(), sell_trades: Vec::new() });
                                    for trade in trades_buffer.drain(..) {
                                        if trade.is_buyer_maker {
                                            trade_groups.sell_trades.push((trade.price, trade.quantity));
                                        } else {
                                            trade_groups.buy_trades.push((trade.price, trade.quantity));
                                        }
                                    }
                                },
                                Err(poisoned) => {
                                    log(&format!("klines_trades locked on render: {:?}", poisoned));
                                }
                            }
                        }
                    },
                    Some(stream) if stream.contains("kline") => {
                        if let Some(kline_data) = v["data"]["k"].as_object() {
                            let open_time = kline_data["t"].as_u64();
                            let open = kline_data["o"].as_str().and_then(|s| s.parse::<f64>().ok());
                            let high = kline_data["h"].as_str().and_then(|s| s.parse::<f64>().ok());
                            let low = kline_data["l"].as_str().and_then(|s| s.parse::<f64>().ok());
                            let close = kline_data["c"].as_str().and_then(|s| s.parse::<f64>().ok());
                            let volume = kline_data["v"].as_str().and_then(|s| s.parse::<f64>().ok());
                            let buy_volume = kline_data["V"].as_str().and_then(|s| s.parse::<f64>().ok());
                            let sell_volume = match (volume, buy_volume) {
                                (Some(volume), Some(buy_volume)) => Some(volume - buy_volume),
                                _ => None,
                            };
                            let close_time = kline_data["T"].as_u64();
                    
                            if let (
                                Some(open_time), 
                                Some(open), Some(high), Some(low), Some(close), 
                                Some(buy_volume), Some(sell_volume), 
                                Some(close_time)) = (open_time, open, high, low, close, buy_volume, sell_volume, close_time) {
                            
                                let mut last_kline_cvd = 0.0;
                                let mut last_open_time = 0;
                                let mut last_buy_volume = 0.0;
                                let mut last_sell_volume = 0.0;
                                match klines_ohlcv.read() {
                                    Ok(klines_ohlcv) => {
                                        if let Some((last_open_time_val, last_kline)) = klines_ohlcv.iter().last() {
                                            last_kline_cvd = last_kline.cum_volume_delta;
                                            last_open_time = *last_open_time_val;
                                            last_buy_volume = last_kline.buy_volume;
                                            last_sell_volume = last_kline.sell_volume;
                                        }
                                    },
                                    Err(e) => {
                                        log(&format!("Failed to acquire lock on klines_ohlcv during render: {}", e));
                                    }
                                };
                                let cum_volume_delta = if last_open_time == open_time {
                                    last_kline_cvd - (last_buy_volume - last_sell_volume) + (buy_volume - sell_volume)
                                } else {
                                    last_kline_cvd + buy_volume - sell_volume
                                };
                            
                                let kline = Kline {
                                    open_time, 
                                    open, high, low, close,
                                    buy_volume, sell_volume,
                                    cum_volume_delta,
                                    close_time,
                                };
                                if let Ok(mut klines_ohlcv) = klines_ohlcv.try_write() {
                                    klines_ohlcv.insert(open_time, kline);  
                                }
                                current_kline_open = open_time;
                            }
                        }
                    },
                    _ => {
                       log(&format!("Unknown stream: {:?}", v));
                    }
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);

        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();

        self.websocket = Some(ws);
    }   

    pub fn render(&mut self) {
        match self.klines_ohlcv.try_read() {
            Ok(klines_borrowed) => {
                let last_kline_open: u64 = match klines_borrowed.iter().last() {
                    Some((last_kline_open, _)) => *last_kline_open,
                    None => return, 
                };
                let zoom_scale: f64 = self.x_zoom * 60.0 * 1000.0;
                let time_difference: f64 = last_kline_open as f64 + 60000.0 - zoom_scale;

                let left_x: f64 = 0.0 - self.pan_x_offset;
                let right_x: f64 = self.canvas_main.width - self.pan_x_offset;

                let visible_klines: Vec<_> = klines_borrowed.iter().filter(|&(open_time, _)| {
                    let x: f64 = ((*open_time as f64) - time_difference) / zoom_scale * self.canvas_main.width;
                    x >= left_x as f64 && x <= right_x as f64
                }).collect();

                let avg_body_length: f64 = visible_klines.iter()
                    .map(|(_, kline)| (kline.close - kline.open).abs())
                    .sum::<f64>() / visible_klines.len() as f64;

                let y_max: f64 = visible_klines.iter().map(|(_, kline)| kline.high).fold(0.0, f64::max) + avg_body_length;
                let y_min: f64 = visible_klines.iter().map(|(_, kline)| kline.low).fold(f64::MAX, f64::min) - avg_body_length;

                self.canvas_indicator_volume.render(&visible_klines);
                
                match self.oi_datapoints.try_read() {
                    Ok(oi_datapoints_borrowed) => {
                        let visible_oi_datapoints: Vec<_> = oi_datapoints_borrowed.iter().filter(|&(time, _)| {
                            let x: f64 = ((*time as f64) - time_difference) / zoom_scale * self.canvas_main.width;
                            x >= left_x as f64 && x <= right_x as f64
                        }).collect();
                        self.canvas_indi_cvd.render(&visible_klines, &visible_oi_datapoints);
                    },
                    Err(e) => {
                        log(&format!("Failed to acquire lock on oi_datapoints during render: {}", e));
                    }
                }

                match (self.orderbook_manager.bids.try_read(), self.orderbook_manager.asks.try_read()) {
                    (Ok(bids_borrowed), Ok(asks_borrowed)) => {
                        let bucket_size = self.bucket_size.read().unwrap();
                        let decimals = self.tick_size.borrow().log10().abs() as i32;
                        let multiplier = 10f64.powi(decimals);

                        let grouped_bids = group_orders(*bucket_size, &bids_borrowed, multiplier);
                        let grouped_asks = group_orders(*bucket_size, &asks_borrowed, multiplier);

                        self.canvas_orderbook.render(y_min, y_max, grouped_bids, grouped_asks, &visible_klines, &self.last_depth_update, decimals);

                        match self.klines_trades.try_read() {
                            Ok(klines_trades_borrowed) => {    
                                let mut grouped_trades: Vec<(u64, GroupedTrades)> = Vec::new();
                        
                                for (open_time, trade_groups) in klines_trades_borrowed.iter() {
                                    let x: f64 = ((*open_time as f64) - time_difference) / zoom_scale * self.canvas_main.width;
                                    if x >= left_x as f64 && x <= right_x as f64 {
                                        let mut buys: HashMap<i64, f64> = HashMap::new();
                                        let mut sells: HashMap<i64, f64> = HashMap::new();
                        
                                        for (price, quantity) in &trade_groups.buy_trades {
                                            let bucket = ((price / *bucket_size).round() * *bucket_size * multiplier) as i64;
                                            let entry = buys.entry(bucket).or_insert(0.0);
                                            *entry += quantity;
                                        }
                                        for (price, quantity) in &trade_groups.sell_trades {
                                            let bucket = ((price / *bucket_size).round() * *bucket_size * multiplier) as i64;
                                            let entry = sells.entry(bucket).or_insert(0.0);
                                            *entry += quantity;
                                        }
                        
                                        grouped_trades.push((*open_time, GroupedTrades { buys, sells }));
                                    }
                                }
                                self.canvas_main.render(y_min, y_max, &visible_klines, grouped_trades, multiplier);
                            },
                            Err(e) => {
                                log(&format!("Failed to acquire lock on klines_trades during render: {}", e));
                            }
                        }
                    },
                    (Err(e), _) => {
                        log(&format!("Failed to acquire lock on bids during render: {}", e));
                    },
                    (_, Err(e)) => {
                        log(&format!("Failed to acquire lock on asks during render: {}", e));
                    }
                }
            },
            Err(e) => {
                log(&format!("Failed to acquire lock on klines during render: {}", e));
            }
        }
    }

    pub fn pan_x(&mut self, x: f64) {
        self.pan_x_offset += x * 1.5;
        if self.pan_x_offset < 0.0 {
            self.pan_x_offset = 0.0;
        }
        self.canvas_main.pan_x_offset = self.pan_x_offset;
        self.canvas_indi_cvd.pan_x_offset = self.pan_x_offset;
        self.canvas_indicator_volume.pan_x_offset = self.pan_x_offset;
    }
    pub fn zoom_x(&mut self, x: f64) {
        let factor = if x > 0.0 { 0.9 } else { 1.1 };
        self.x_zoom *= factor;
        self.x_zoom = self.x_zoom.round(); 
        if self.x_zoom < 3.0 {
            self.x_zoom = 3.0;
        }
        if self.x_zoom > 40.0 {
            self.x_zoom = 40.0;
        }
        self.canvas_main.x_zoom = self.x_zoom;
        self.canvas_indi_cvd.x_zoom = self.x_zoom;
        self.canvas_indicator_volume.x_zoom = self.x_zoom;
    }    

    pub fn resize(&mut self, new_widths: &[f64], new_heights: &[f64]) {
        self.canvas_main.resize(new_widths[0], new_heights[0]);
        self.canvas_orderbook.resize(new_widths[1], new_heights[1]);
        self.canvas_indicator_volume.resize(new_widths[2], new_heights[2]);
        self.canvas_bubble.borrow_mut().resize(new_widths[3], new_heights[3]);
        self.canvas_indi_cvd.resize(new_widths[4], new_heights[4]);
    }

    pub fn gather_depth(&mut self, depth: JsValue) {
        self.orderbook_manager.fetch_depth(depth);
    }
    pub fn gather_oi(&mut self, oi: JsValue) {
        if let Some(oi_str) = oi.as_string() {
            match serde_json::from_str::<serde_json::Value>(&oi_str) {
                Ok(oi_obj) => {
                    if let (Some(time), Some(open_interest_str)) = (oi_obj["time"].as_u64(), oi_obj["openInterest"].as_str()) {
                        if let Ok(open_interest) = open_interest_str.parse::<f64>() {
                            match self.oi_datapoints.try_write() {
                                Ok(mut oi_datapoints) => oi_datapoints.push((time, open_interest)),
                                Err(_) => println!("Failed to acquire write lock on oi_datapoints"),
                            };
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Failed to parse oi: {}", e);
                }
            }
        }
    }
    pub fn gather_hist_oi(&mut self, hist_ois: JsValue) {
        if let Some(hist_ois_str) = hist_ois.as_string() {
            match serde_json::from_str::<Vec<serde_json::Value>>(&hist_ois_str) {
                Ok(hist_ois) => {
                    for hist_oi in hist_ois {
                        if let (Some(time), Some(open_interest_str)) = (hist_oi["timestamp"].as_u64(), hist_oi["sumOpenInterest"].as_str()) {
                            if let Ok(open_interest) = open_interest_str.parse::<f64>() {
                                match self.oi_datapoints.try_write() {
                                    Ok(mut oi_datapoints) => oi_datapoints.push((time, open_interest)),
                                    Err(_) => println!("Failed to acquire write lock on oi_datapoints"),
                                };
                            }
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Failed to parse hist_ois: {}", e);
                }
            }
        }
    }       
    pub fn gather_klines(&mut self, klines: JsValue) {
        if let Some(klines_str) = klines.as_string() {
            match serde_json::from_str::<Vec<Vec<serde_json::Value>>>(&klines_str) {
                Ok(klines) => {
                    if let Ok(mut klines_ohlcv) = self.klines_ohlcv.try_write() {
                        let mut cum_volume_delta = 0.0;
                        for kline in klines {
                            if let (
                                Some(open_time), 
                                Some(open_str), Some(high_str), Some(low_str), Some(close_str), 
                                Some(volume_str), Some(close_time), Some(buy_volume_str)) = 
                                (
                                    kline[0].as_u64(), 
                                    kline[1].as_str(), kline[2].as_str(), kline[3].as_str(), kline[4].as_str(), 
                                    kline[5].as_str(), kline[6].as_u64(), kline[9].as_str()
                                ) {
                                if let (Ok(open), Ok(high), Ok(low), Ok(close), Ok(volume), Ok(buy_volume)) = 
                                    (
                                        open_str.parse::<f64>(), high_str.parse::<f64>(), low_str.parse::<f64>(), close_str.parse::<f64>(), 
                                        volume_str.parse::<f64>(), buy_volume_str.parse::<f64>()
                                    ) {
                                    let sell_volume = volume - buy_volume;
                                    cum_volume_delta += buy_volume - sell_volume;
                                    let kline = Kline {
                                        open_time, 
                                        open, high, low, close, 
                                        buy_volume, sell_volume, 
                                        cum_volume_delta,
                                        close_time,
                                    };
                                    klines_ohlcv.insert(open_time, kline);
                                }
                            }
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Failed to parse klines: {}", e);
                }
            }
        }
    }    
    pub fn gather_hist_trades(&mut self, hist_trades: JsValue, i: String) {
        let i = match i.parse::<u64>() {
            Ok(val) => val,
            Err(_) => {
                log("Failed to parse i as u64");
                return;
            }
        };
        if let Some(hist_trades_str) = hist_trades.as_string() {
            match serde_json::from_str::<Vec<Trade>>(&hist_trades_str) {
                Ok(hist_trades) => {
                    match self.klines_trades.try_write() {
                        Ok(mut klines_trades) => {
                            let mut trade_groups = TradeGroups { buy_trades: Vec::new(), sell_trades: Vec::new() };
                            for trade in hist_trades {
                                if trade.is_buyer_maker {
                                    trade_groups.sell_trades.push((trade.price, trade.quantity));
                                } else {
                                    trade_groups.buy_trades.push((trade.price, trade.quantity));
                                }
                            }
                            klines_trades.insert(i, trade_groups);
                        },
                        Err(poisoned) => {
                            log(&format!("klines_trades locked on render: {:?}", poisoned));
                        }
                    }
                },
                Err(e) => {
                    log(&format!("Failed to parse hist_trades: {}", e));
                }
            }
        }
    }

    pub fn set_symbol_info(&mut self, default_tick_size: f64, _min_trade_size: f64, user_tick_setting: f64) {
        if let Ok(mut bucket_size) = self.bucket_size.try_write() {
            *bucket_size = default_tick_size * user_tick_setting;
            *self.tick_size.borrow_mut() = default_tick_size;
            log(&format!("Default bucket size: {}", *bucket_size));
        }
    }
    pub fn set_tick_size(&mut self, user_tick_setting: f64) {
        if let Ok(mut bucket_size) = self.bucket_size.try_write() {
            *bucket_size = user_tick_setting * *self.tick_size.borrow();
            log(&format!("Setting bucket size to: {}", *bucket_size));
        }
    }
    pub fn get_kline_ohlcv_keys(&self) -> Vec<u64> {
        match self.klines_ohlcv.try_read() {
            Ok(klines_borrowed) => klines_borrowed.keys().cloned().collect(),
            Err(e) => {
                log(&format!("Failed to acquire lock on klines_ohlcv during get_kline_ohlcv_keys: {}", e));
                Vec::new()
            }
        }
    }

    pub fn clear_datasets(&mut self) {
        self.klines_ohlcv.write().unwrap().clear();
        self.klines_trades.write().unwrap().clear();
        self.oi_datapoints.write().unwrap().clear();
        self.canvas_bubble.borrow_mut().reset();
    }
}
pub struct CanvasOrderbook {
    ctx: CanvasRenderingContext2d,
    width: f64,
    height: f64,
}
impl CanvasOrderbook {
    pub fn new(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let width = canvas.width() as f64;
        let height = canvas.height() as f64;

        match canvas.get_context("2d") {
            Ok(Some(context)) => {
                let ctx = context.dyn_into::<CanvasRenderingContext2d>()?;
                Ok(Self {
                    ctx,
                    width,
                    height,
                })
            },
            Ok(None) => Err(JsValue::from_str("No 2D context available")),
            Err(error) => Err(error),
        }
    }
    pub fn resize(&mut self, new_width: f64, new_height: f64) {
        self.width = new_width;
        self.height = new_height;
    }

    pub fn render(&mut self, y_min: f64, y_max: f64, bids: Vec<Order>, asks: Vec<Order>, klines: &Vec<(&u64, &Kline)>, last_depth_update: &Rc<RefCell<u64>>, decimals: i32) {
        let context = &self.ctx;
        self.ctx.clear_rect(0.0, 0.0, self.width, self.height);

        let max_quantity = {
            let max_bid_quantity = bids.iter().fold(0.0, |max, x| x.quantity.max(max));
            let max_ask_quantity = asks.iter().fold(0.0, |max, x| x.quantity.max(max));
            max_bid_quantity.max(max_ask_quantity)
        }; 

        let num_labels = 12; 
        let step = (y_max - y_min) / num_labels as f64;
        context.set_font("20px monospace");
        context.set_fill_style(&"rgba(200, 200, 200, 0.8)".into());
        
        for i in 0..=num_labels {
            let y_value = y_min + step * i as f64;
            let y = self.height - ((y_value - y_min) / (y_max - y_min)) * self.height as f64;

            let y_value_str = format!("{:.*}", decimals as usize, y_value);

            context.fill_text(&y_value_str, 6.0, y).unwrap();
        }

        if let Some(_best_bid) = bids.first() {     
            context.set_stroke_style(&"rgba(81, 205, 160, 1)".into());
            for (_i, bid) in bids.iter().enumerate() {
                let x = (bid.quantity / (max_quantity + max_quantity/4.0)) * (self.width - 120.0) as f64;
                let y = ((bid.price - y_min) / (y_max - y_min)) * self.height as f64;
                context.begin_path();
                context.move_to(108.0, self.height - y);
                context.line_to(108.0 + x, self.height - y);
                context.stroke();
            }
        }
        if let Some(_best_ask) = asks.first() {
            context.set_stroke_style(&"rgba(192, 80, 77, 1)".into());
            for (_i, ask) in asks.iter().enumerate() {
                let x = (ask.quantity / (max_quantity + max_quantity/4.0)) * (self.width - 120.0) as f64;
                let y = ((ask.price - y_min) / (y_max - y_min)) * self.height as f64;
                context.begin_path();
                context.move_to(108.0, self.height - y);
                context.line_to(108.0 + x, self.height - y);
                context.stroke();
            }

        }
        klines.last().map(|(_last_time, kline)| {
            let y = ((kline.close - y_min) / (y_max - y_min)) * self.height as f64;
            let y_value_str = format!("{:.*}", decimals as usize, kline.close);

            if kline.open < kline.close {
                context.set_fill_style(&"rgba(81, 205, 160, 1)".into());
            } else {
                context.set_fill_style(&"rgba(192, 80, 77, 1)".into());
            }
            let rect_y = self.height - y - 40.0;
            context.fill_rect(3.0, rect_y + 20.0, 90.0, 50.0); 

            context.set_fill_style(&"black".into());
            context.fill_text(&y_value_str, 6.0, self.height - y).unwrap();

            let time_left = if kline.close_time < *last_depth_update.borrow() {
                0
            } else {
                (kline.close_time - *last_depth_update.borrow()) / 1000
            };
            let time_left_str = format!("{:02}:{:02}", time_left / 60, time_left % 60);
            context.set_font("100 16px monospace");
            context.fill_text(&time_left_str, 6.0, self.height - y + 20.0).unwrap(); 
        });
        let max_quantity_str = format!("{:.1}", max_quantity);
        context.set_fill_style(&"rgba(200, 200, 200, 0.8)".into());
        context.set_font("18px monospace");
        context.fill_text(&max_quantity_str, self.width - 100.0, 20.0).unwrap();
    } 
}
pub struct CanvasMain {
    ctx: CanvasRenderingContext2d,
    width: f64,
    height: f64,
    x_zoom: f64,
    pan_x_offset: f64,
}
impl CanvasMain {
    pub fn new(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let width = canvas.width() as f64;
        let height = canvas.height() as f64;

        match canvas.get_context("2d") {
            Ok(Some(context)) => {
                let ctx = context.dyn_into::<CanvasRenderingContext2d>()?;
                Ok(Self {
                    ctx,
                    width,
                    height,
                    x_zoom: 30.0,
                    pan_x_offset: 0.0,
                })
            },
            Ok(None) => Err(JsValue::from_str("No 2D context available")),
            Err(error) => Err(error),
        }
    }
    pub fn resize(&mut self, new_width: f64, new_height: f64) {
        self.width = new_width;
        self.height = new_height;
    }

    pub fn render(&mut self, y_min: f64, y_max: f64, klines: &Vec<(&u64, &Kline)>, trades: Vec<(u64, GroupedTrades)>, multiplier: f64) {
        let context = &self.ctx;
        context.clear_rect(0.0, 0.0, self.width, self.height);
        
        if let Some((last_kline_open, _)) = klines.iter().last() {
            let zoom_scale = self.x_zoom * 60.0 * 1000.0;
            let time_difference: f64 = **last_kline_open as f64 + 60000.0 - zoom_scale;
            let rect_width: f64 = (self.width as f64 / &self.x_zoom)/2.0;
            
            let max_quantity = trades.iter().flat_map(|(_, trade_groups)| {
                trade_groups.buys.iter().chain(trade_groups.sells.iter()).map(|(_, quantity)| *quantity)
            }).fold(0.0, f64::max);
    
            context.set_line_width(1.0);
            for (_i, (_, kline)) in klines.iter().enumerate() {
                let x: f64 = ((kline.open_time as f64 - time_difference) as f64 / zoom_scale) * self.width;

                let y_open = self.height as f64 * (kline.open - y_min) / (y_max - y_min);
                let y_close = self.height as f64 * (kline.close - y_min) / (y_max - y_min);
                let y_high = self.height as f64 * (kline.high - y_min) / (y_max - y_min);
                let y_low = self.height as f64 * (kline.low - y_min) / (y_max - y_min);

                context.set_stroke_style(&(if kline.open < kline.close { "rgba(50, 200, 50, 1)" } else { "rgba(200, 50, 50, 1)" }).into());
                context.begin_path();
                context.move_to(x + rect_width, self.height - y_open);
                context.line_to(x + rect_width, self.height - y_close);
                context.stroke();

                context.set_stroke_style(&"rgba(200, 200, 200, 0.5)".into());
                context.begin_path();
                context.move_to(x, self.height - y_high);
                context.line_to(x + (rect_width*2.0), self.height - y_high);

                context.move_to(x, self.height - y_low);
                context.line_to(x + (rect_width*2.0), self.height - y_low);
                context.stroke();

                if let Some((_, trade_groups)) = trades.iter().find(|&&(time, _)| time == kline.open_time) {
                    context.set_stroke_style(&"rgba(81, 205, 160, 1)".into());
                    for (price_as_int, quantity) in &trade_groups.buys { 
                        let price = *price_as_int as f64 / multiplier;
                        let y_trade = self.height as f64 * (price - y_min) / (y_max - y_min);
                        let scaled_quantity = rect_width as f64 * quantity / max_quantity;

                        context.begin_path();
                        context.move_to(x + rect_width + 4.0, self.height - y_trade);
                        context.line_to(x + rect_width + 4.0 + scaled_quantity, self.height - y_trade);
                        context.stroke();
                    }
                    context.set_stroke_style(&"rgba(192, 80, 77, 1)".into());
                    for (price_as_int, quantity) in &trade_groups.sells {
                        let price = *price_as_int as f64 / multiplier;
                        let y_trade = self.height as f64 * (price - y_min) / (y_max - y_min);
                        let scaled_quantity = rect_width as f64 * quantity / max_quantity;

                        context.begin_path();
                        context.move_to(x + rect_width - 4.0, self.height - y_trade);
                        context.line_to(x + rect_width - 4.0 - scaled_quantity, self.height - y_trade);
                        context.stroke();
                    }
                }
            }
        }
    }
}
pub struct CanvasIndicatorVolume {
    ctx: CanvasRenderingContext2d,
    width: f64,
    height: f64,
    x_zoom: f64,
    pan_x_offset: f64,
}
impl CanvasIndicatorVolume {
    pub fn new(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let width = canvas.width() as f64;
        let height = canvas.height() as f64;

        match canvas.get_context("2d") {
            Ok(Some(context)) => {
                let ctx = context.dyn_into::<CanvasRenderingContext2d>()?;
                Ok(Self {
                    ctx,
                    width,
                    height,
                    x_zoom: 30.0,
                    pan_x_offset: 0.0,
                })
            },
            Ok(None) => Err(JsValue::from_str("No 2D context available")),
            Err(error) => Err(error),
        }
    }
    pub fn resize(&mut self, new_width: f64, new_height: f64) {
        self.width = new_width;
        self.height = new_height;
    }

    pub fn render(&mut self, klines: &Vec<(&u64, &Kline)>) {
        let context = &self.ctx;
        context.clear_rect(0.0, 0.0, self.width, self.height);
        
        let zoom_scale = self.x_zoom * 60.0 * 1000.0;
        let rect_width: f64 = (self.width as f64 / &self.x_zoom)/2.0;

        match klines.iter().last() {
            Some((last_kline_open, _)) => {
                let max_volume = klines.iter().map(|(_, kline)| f64::max(kline.buy_volume, kline.sell_volume)).fold(0.0, f64::max);
                let time_difference = **last_kline_open as f64 + 60000.0 - zoom_scale;

                for (_i, (_, kline)) in klines.iter().enumerate() {
                    let x = ((kline.open_time as f64 - time_difference) as f64 / zoom_scale) * self.width;
                
                    let buy_height = self.height as f64 * (kline.buy_volume / max_volume);
                    let sell_height = self.height as f64 * (kline.sell_volume / max_volume);
                
                    context.set_fill_style(&"rgba(81, 205, 160, 1)".into());
                    context.fill_rect(x + rect_width, self.height as f64 - buy_height, rect_width - 10.0, buy_height);
                
                    context.set_fill_style(&"rgba(192, 80, 77, 1)".into());
                    context.fill_rect(x + 10.0, self.height as f64 - sell_height, rect_width - 10.0, sell_height);
                }
            },
            None => {
                log(&format!("No klines"));
            }
        }
    }
}
pub struct CanvasIndiCVD {
    ctx: CanvasRenderingContext2d,
    width: f64,
    height: f64,
    x_zoom: f64,
    pan_x_offset: f64,
}
impl CanvasIndiCVD {
    pub fn new(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let width = canvas.width() as f64;
        let height = canvas.height() as f64;

        match canvas.get_context("2d") {
            Ok(Some(context)) => {
                let ctx = context.dyn_into::<CanvasRenderingContext2d>()?;
                Ok(Self {
                    ctx,
                    width,
                    height,
                    x_zoom: 30.0,
                    pan_x_offset: 0.0,
                })
            },
            Ok(None) => Err(JsValue::from_str("No 2D context available")),
            Err(error) => Err(error),
        }
    }
    pub fn resize(&mut self, new_width: f64, new_height: f64) {
        self.width = new_width;
        self.height = new_height;
    }

    pub fn render(&mut self, klines: &Vec<(&u64, &Kline)>, oi_obj: &Vec<&(u64, f64)>) {
        let context = &self.ctx;
        context.clear_rect(0.0, 0.0, self.width, self.height);
    
        let zoom_scale = self.x_zoom * 60.0 * 1000.0;
        
        match klines.iter().last() {
            Some((last_kline_open, _)) => {
                let max_cvd = klines.iter().map(|(_, kline)| kline.cum_volume_delta).fold(0.0, f64::max);
                let min_cvd = klines.iter().map(|(_, kline)| kline.cum_volume_delta).fold(f64::MAX, f64::min);

                let time_difference = *last_kline_open + 60000 - zoom_scale as u64;
                let rect_width: f64 = (self.width as f64 / &self.x_zoom)/2.0;

                let mut previous_point: Option<(f64, f64)> = None;
                context.set_stroke_style(&"rgba(238, 216, 139, 0.4)".into());
                for (_i, (_, kline)) in klines.iter().enumerate() {
                    let x = ((kline.open_time - time_difference) as f64 / zoom_scale) * self.width;
                    let y = self.height as f64 * (kline.cum_volume_delta - min_cvd) / (max_cvd - min_cvd);
            
                    match previous_point {
                        Some((prev_x, prev_y)) => {
                            context.begin_path();
                            context.move_to(prev_x + (rect_width*2.0), self.height - prev_y);
                            context.line_to(x + (rect_width*2.0), self.height - y);
                            context.stroke();
                        },
                        None => (),
                    }
                    previous_point = Some((x, y));
                }

                match oi_obj.iter().last() {
                    Some((_oi_last_time, _)) => {
                        let max_oi = oi_obj.iter().map(|(_, oi)| *oi).fold(0.0, f64::max);
                        let min_oi = oi_obj.iter().map(|(_, oi)| *oi).fold(f64::MAX, f64::min);
        
                        let time_difference = *last_kline_open + 60000 - zoom_scale as u64;
                        let padding_ratio = 0.1; 
                        let padded_height = self.height as f64 * (1.0 - padding_ratio);
                        let padding = self.height as f64 * padding_ratio / 2.0;
            
                        context.set_fill_style(&"white".into());
                        for (_i, (time, oi)) in oi_obj.iter().enumerate() {
                            let x = ((time - time_difference) as f64 / zoom_scale) * self.width;
                            let y = if max_oi == min_oi {
                                padded_height / 2.0 + padding
                            } else {
                                padded_height * (*oi - min_oi) / (max_oi - min_oi) + padding
                            };
                            
                            context.begin_path();
                            context.arc(x, self.height - y, 4.0, 0.0, 2.0 * std::f64::consts::PI).unwrap();
                            context.fill();
                        }
                    },
                    None => {
                    }
                }
            },
            None => {
                log(&format!("No klines"));
            }
        }
    }
}
pub struct CanvasBubbleTrades {
    ctx: CanvasRenderingContext2d,
    width: f64,
    height: f64,
    trades: Vec<Trade>,
    sell_trade_counts: BTreeMap<u64, usize>,
    buy_trade_counts: BTreeMap<u64, usize>,
}
impl CanvasBubbleTrades {
    pub fn new(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let width = canvas.width() as f64;
        let height = canvas.height() as f64;

        match canvas.get_context("2d") {
            Ok(Some(context)) => {
                let ctx = context.dyn_into::<CanvasRenderingContext2d>()?;
                Ok(Self {
                    ctx,
                    width,
                    height,
                    trades: Vec::new(),
                    sell_trade_counts: BTreeMap::new(),
                    buy_trade_counts: BTreeMap::new(),
                })
            },
            Ok(None) => Err(JsValue::from_str("No 2D context available")),
            Err(error) => Err(error),
        }
    }
    pub fn resize(&mut self, new_width: f64, new_height: f64) {
        self.width = new_width;
        self.height = new_height;
    }
    pub fn reset(&mut self) {
        self.trades.clear();
        self.sell_trade_counts.clear();
        self.buy_trade_counts.clear();
    }

    pub fn render(&mut self, trades_buffer: &Vec<Trade>, last_update: u64) {
        let context = &self.ctx;
        context.clear_rect(0.0, 0.0, self.width, self.height);

        let padding_percentage = 0.25; 
        let padded_height = self.height * (1.0 - padding_percentage);
        let thirty_seconds_ago = last_update - 30 * 1000;

        // Trade counts
        self.buy_trade_counts.retain(|&time, _| time >= thirty_seconds_ago);
        self.sell_trade_counts.retain(|&time, _| time >= thirty_seconds_ago);

        let sell_count = trades_buffer.iter()
            .filter(|trade| trade.is_buyer_maker)
            .count();
        self.sell_trade_counts.insert(last_update, sell_count);
        let buy_count = trades_buffer.iter()
            .filter(|trade| !trade.is_buyer_maker)
            .count();
        self.buy_trade_counts.insert(last_update, buy_count);

        let max_trade_count = self.sell_trade_counts.iter().chain(self.buy_trade_counts.iter())
            .map(|(_, &count)| count)
            .fold(0, usize::max);
        let y_scale = padded_height / (2 * max_trade_count) as f64;

        context.set_stroke_style(&"rgba(200, 50, 50, 0.4)".into());
        let mut previous_point: Option<(f64, f64)> = None;
        for (&time, &count) in self.sell_trade_counts.iter() {
            let x = ((time - thirty_seconds_ago) as f64 / 30000.0) * self.width;
            let y = self.height * padding_percentage / 2.0 + padded_height / 2.0 + count as f64 * y_scale;
            match previous_point {
                Some((prev_x, prev_y)) => {
                    context.begin_path();
                    context.move_to(prev_x, prev_y);
                    context.line_to(x, y);
                    context.stroke();
                },
                None => (),
            }
            previous_point = Some((x, y));
        }
        context.set_stroke_style(&"rgba(50, 200, 50, 0.4)".into());
        previous_point = None;
        for (&time, &count) in self.buy_trade_counts.iter() {
            let x = ((time - thirty_seconds_ago) as f64 / 30000.0) * self.width;
            let y = self.height * padding_percentage / 2.0 + padded_height / 2.0 - count as f64 * y_scale;
            match previous_point {
                Some((prev_x, prev_y)) => {
                    context.begin_path();
                    context.move_to(prev_x, prev_y);
                    context.line_to(x, y);
                    context.stroke();
                },
                None => (),
            }
            previous_point = Some((x, y));
        }

        // Bubble trades
        self.trades.retain(|trade| trade.time >= thirty_seconds_ago);

        for trade in trades_buffer {
            self.trades.push(*trade);
        }
        
        let max_quantity = self.trades.iter().map(|trade| trade.quantity).fold(0.0, f64::max);
        let max_radius = 35.0;
        let y_min = self.trades.iter().map(|trade| trade.price).fold(f64::MAX, f64::min);
        let y_max = self.trades.iter().map(|trade| trade.price).fold(0.0, f64::max);

        let sell_trades: Vec<_> = self.trades.iter().filter(|trade| trade.is_buyer_maker).collect();
        let buy_trades: Vec<_> = self.trades.iter().filter(|trade| !trade.is_buyer_maker).collect();

        context.set_fill_style(&"rgba(192, 80, 77, 1)".into());
        for trade in &sell_trades {
            let radius = ((trade.quantity / max_quantity) * 40.0).min(max_radius);    
            if radius > 1.0 {
                let x = ((trade.time - thirty_seconds_ago) as f64 / 30000.0) * self.width;
                let y = ((trade.price - y_min) / (y_max - y_min)) * padded_height + self.height * padding_percentage / 2.0;

                context.begin_path();
                context.arc(x, self.height - y, radius, 0.0, 2.0 * std::f64::consts::PI).unwrap();
                context.fill();
            }
        }
        context.set_fill_style(&"rgba(81, 205, 160, 1)".into());
        for trade in &buy_trades {
            let radius = ((trade.quantity / max_quantity) * 40.0).min(max_radius);
            if radius > 1.0 {
                let x = ((trade.time - thirty_seconds_ago) as f64 / 30000.0) * self.width;
                let y = ((trade.price - y_min) / (y_max - y_min)) * padded_height + self.height * padding_percentage / 2.0;

                context.begin_path();
                context.arc(x, self.height - y, radius, 0.0, 2.0 * std::f64::consts::PI).unwrap();
                context.fill();
            }
        }
    }
}

pub fn group_orders(bucket_size: f64, orders: &Vec<Order>, multiplier: f64) -> Vec<Order> {
    let mut grouped_orders = HashMap::new();
    for order in orders {
        let price = ((order.price / bucket_size).round() * bucket_size * multiplier) as i64;
        let quantity = grouped_orders.entry(price).or_insert(0.0);
        *quantity += order.quantity;
    }
    let mut orders = Vec::new();
    for (price, quantity) in grouped_orders {
        let price = price as f64 / multiplier; 
        orders.push(Order { price, quantity });
    }
    orders
}
pub struct OrderbookManager {
    bids: Arc<RwLock<Vec<Order>>>,
    asks: Arc<RwLock<Vec<Order>>>,
    last_update_id: Arc<RwLock<u64>>,
}
impl OrderbookManager {
    pub fn new() -> Self {
        Self {
            bids: Arc::new(RwLock::new(Vec::new())),
            asks: Arc::new(RwLock::new(Vec::new())),
            last_update_id: Arc::new(RwLock::new(0)),
        }
    }
    pub fn fetch_depth(&mut self, depth: JsValue) {
        if let Some(depth_str) = depth.as_string() {
            match serde_json::from_str::<serde_json::Value>(&depth_str) {
                Ok(depth) => {
                    if let Some(bids_array) = depth["bids"].as_array() {
                        if let Ok(mut bids_borrowed) = self.bids.try_write() {
                            *bids_borrowed = bids_array.iter().filter_map(|x| {
                                x[0].as_str().and_then(|price_str| price_str.parse::<f64>().ok())
                                    .and_then(|price| x[1].as_str().and_then(|quantity_str| quantity_str.parse::<f64>().ok())
                                    .map(|quantity| Some(Order { price, quantity })).flatten())
                            }).collect();
                        } else {
                            log(&format!("bids locked on render"));
                        }
                    }     
                    if let Some(asks_array) = depth["asks"].as_array() {
                        if let Ok(mut asks_borrowed) = self.asks.try_write() {
                            *asks_borrowed = asks_array.iter().filter_map(|x| {
                                x[0].as_str().and_then(|price_str| price_str.parse::<f64>().ok())
                                    .and_then(|price| x[1].as_str().and_then(|quantity_str| quantity_str.parse::<f64>().ok())
                                    .map(|quantity| Some(Order { price, quantity })).flatten())
                            }).collect();
                        } else {
                            log(&format!("asks locked on render"));
                        }
                    }

                    if let Ok(mut last_update_id) = self.last_update_id.write() {
                        if let Some(last_update_id_val) = depth["lastUpdateId"].as_u64() {
                            *last_update_id = last_update_id_val;
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Failed to parse depth: {}", e);
                }
            }
        }
    }
}