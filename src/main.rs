extern crate rand;
extern crate slab;

use rand::Rng;
use slab::Slab;

#[derive(Debug, PartialEq, Eq)]
enum Knowing {
    Yes,
    No,
    Unknown,
}

#[derive(Debug, Clone, Copy)]
struct Node {
    li: usize,      // The index of the node to the left
    ri: usize,      // ... to the right
    ui: usize,      // ... up
    di: usize,      // ... down
    ci: usize,      // The index of the column header
    size: usize,    // The number of nodes in the column (only use for column headers)
    id: [usize; 3], // The row, column, and number represented by the node in the Sudoku puzzle
    i: usize,       // The index of the node, so it can tell others
}

// Store the dancing links
//
// The links are stored in a slab. The first entry is `h`, the handle on the column headers.
// Following `h` are all the column headers, and then all the links.
//
// Because of this structure, the column headers can be referenced directly by index on the range
// `(1..=self.constraints())`.
//
#[derive(Debug)]
struct SudokuWeb {
    slab: Slab<Node>,
    solution: Vec<[usize; 3]>,
    prop_solution: Vec<[usize; 3]>,
    solution_count: usize,
    belts: usize,
    curtains: usize,
    uniq: Knowing,
    possible: Knowing,
}

impl Node {
    fn new_h() -> Self {
        Node {
            li: 0,
            ri: 0,
            ui: 0,
            di: 0,
            ci: 0,
            i: 0,
            size: 0,
            id: [0, 0, 0],
        }
    }

    fn new_column_header(i: usize) -> Self {
        Node {
            li: i - 1,
            ri: 0,
            ui: i,
            di: i,
            ci: i,
            i: i,
            size: 0,
            id: [0, 0, 0],
        }
    }

    fn new_link(
        li: usize,
        ri: usize,
        ui: usize,
        di: usize,
        ci: usize,
        i: usize,
        id: [usize; 3],
    ) -> Self {
        Node {
            li,
            ri,
            ui,
            di,
            ci,
            i,
            id,
            size: 0,
        }
    }
}

impl SudokuWeb {
    // `belts`: The number of rows of big boxes (each with the same number of rows of individual
    // cells).
    // `curtains`: The number of columns of big boxes (each with the same number of columns of
    // individual cells).
    //
    // For a regular sudoku, call `new(3, 3)`.
    //
    // This prepares the slab and populates it with nodes, in preparation for their dance.
    //
    fn new(belts: usize, curtains: usize) -> Self {
        let mut sw = Self {
            slab: Slab::new(),
            solution: vec![],
            prop_solution: vec![],
            solution_count: 0,
            uniq: Knowing::Unknown,
            possible: Knowing::Unknown,
            belts,
            curtains,
        };

        let capacity = sw.capacity();
        sw.slab.reserve_exact(capacity);

        sw.set_h();
        sw.arrange();
        sw
    }

    fn symbols(&self) -> usize {
        self.belts * self.curtains
    }

    fn constraints(&self) -> usize {
        self.symbols() * self.symbols() * 4
    }

    fn possibilities(&self) -> usize {
        self.symbols() * self.symbols() * self.symbols()
    }

    fn nodes(&self) -> usize {
        self.possibilities() * 4
    }

    fn capacity(&self) -> usize {
        1 + self.constraints() + self.nodes()
    }

    fn set_h(&mut self) {
        // h is used as a reference to the list of headers. It also guarantees that the list of
        // headers will be circular, even when all headers have been removed. It is always the
        // first element in the slab.
        let h = Node::new_h();
        self.slab.insert(h);
    }

    // Prepare the links for their dance
    //
    // Create all the column headers and populate all the columns
    //
    fn arrange(&mut self) {
        self.populate_headers();
        self.populate_rows();
    }

    fn populate_headers(&mut self) {
        let nums = self.symbols();
        let cases = ["cell", "row", "column", "block"];
        for (case_index, _case_name) in cases.iter().enumerate() {
            for i in 1..=nums {
                for j in 1..=nums {
                    let key = {
                        let n = case_index * nums * nums + (i - 1) * nums + j;

                        let entry = self.slab.vacant_entry();
                        let key = entry.key();

                        let header = Node::new_column_header(n);
                        entry.insert(header);
                        key
                    };
                    self.set_new_links(key);
                }
            }
        }
    }

    fn populate_rows(&mut self) {
        let nums = self.symbols();

        for r in 1..=nums {
            for c in 1..=nums {
                for n in 1..=nums {
                    self.insert_row(r, c, n);
                }
            }
        }
    }

    fn indices_from_rcn(&self, r: usize, c: usize, n: usize) -> Vec<usize> {
        let nums = self.symbols();
        let block = ((r - 1) / self.curtains) * self.curtains + ((c - 1) / self.belts) + 1;

        let cell_constraint = (r - 1) * nums + c;
        let row_constraint = nums * nums + (r - 1) * nums + n;
        let col_constraint = 2 * nums * nums + (c - 1) * nums + n;
        let block_constraint = 3 * nums * nums + (block - 1) * nums + n;

        // for i in 1..=(4*nums*nums) {
        //     let a = 1 + ((i-1) % nums);
        //     if i == cell_constraint || i == row_constraint || i == col_constraint || i == block_constraint {
        //         print!("{}", a);
        //     } else {
        //         print!(" ");
        //     }
        //     if i % (nums*nums) == 0 {
        //         print!("|");
        //     }
        // }
        // print!("\n");

        vec![
            cell_constraint,
            row_constraint,
            col_constraint,
            block_constraint,
        ]
    }

    // Given the key to a freshly created node, make sure it's neighbors point to it
    fn set_new_links(&mut self, new_key: usize) {
        let node_is = self.at(new_key);

        self.slab[node_is.li].ri = new_key;
        self.slab[node_is.ri].li = new_key;
        self.slab[node_is.ui].di = new_key;
        self.slab[node_is.di].ui = new_key;

        if new_key != node_is.ci {
            self.slab[node_is.ci].size += 1;
        }
    }

    fn insert_row(&mut self, r: usize, c: usize, n: usize) {
        let id = [r, c, n];
        let indices = self.indices_from_rcn(r, c, n);
        let i_first = indices[0];
        let first_key = {
            let ui = self.slab[i_first].ui;

            let entry = self.slab.vacant_entry();
            let key = entry.key();

            let new = Node::new_link(key, key, ui, i_first, i_first, key, id);
            entry.insert(new);
            key
        };
        self.set_new_links(first_key);

        for i in indices.iter().skip(1) {
            let new_key = {
                let li = self.slab[first_key].li;
                let ui = self.slab[*i].ui;

                let entry = self.slab.vacant_entry();
                let key = entry.key();

                let new = Node::new_link(li, first_key, ui, *i, *i, key, id);
                entry.insert(new);
                key
            };

            self.set_new_links(new_key);
        }
    }

    fn at(&self, i: usize) -> Node {
        *self.slab.get(i).unwrap()
    }

    // seek: Don't stop until this many solutions are found, or until there are no more solutions
    // print: Whether to print the solution (before & after) or not
    // rand: pick columns randomly (good for generating puzzles, not for solving)
    fn solve(&mut self, seek: usize, print: bool, rand: bool) {
        self.solution = vec![];
        self.solution_count = 0;

        let ps = self.prop_solution.clone();
        self.possible = self.pre_dance(&ps);

        if self.possible != Knowing::No {
            self.dance(0, seek, print, rand);
            self.post_dance(&ps);
        }
    }

    fn pre_dance(&mut self, ps: &Vec<[usize; 3]>) -> Knowing {
        for (psi, id) in ps.iter().enumerate() {
            let [r, c, n] = id;
            let indices = self.indices_from_rcn(*r, *c, *n);
            'indices: for (ind_i, i) in indices.iter().enumerate() {
                let h = self.at(0);
                let mut j = self.at(h.ri);
                // Make sure that the column to cover is not already covered by checking it is
                // connected to the h node.
                while j.i != h.i {
                    if j.i == *i {
                        self.cover_column(&j);
                        continue 'indices;
                    }
                    j = self.at(j.ri);
                }
                for ri in indices[..ind_i].iter().rev() {
                    let j = self.at(*ri);
                    self.uncover_column(&j);
                }
                self.post_dance(&ps[..psi].to_vec());
                return Knowing::No;
            }
        }
        Knowing::Unknown
    }

    fn post_dance(&mut self, ps: &Vec<[usize; 3]>) {
        for id in ps.iter().rev() {
            let [r, c, n] = id;
            let indices = self.indices_from_rcn(*r, *c, *n);
            for i in indices.iter().rev() {
                let c = self.at(*i);
                self.uncover_column(&c);
            }
        }
    }

    // k: Which iteration we are on
    fn dance(&mut self, k: usize, seek: usize, print: bool, rand: bool) {
        if self.at(0).ri == 0 {
            self.possible = Knowing::Yes;
            self.solution_count += 1;
            self.solution = self.prop_solution.clone();
            if print {
                println!("[{}]: Solution found:", self.solution_count);
                self.print_solution(&self.prop_solution);
            }
            if self.solution_count > 1 {
                self.uniq = Knowing::No;
            }
            return;
        }

        let c = self.choose_column(rand);

        self.cover_column(&c);

        let mut r = self.at(c.di);
        while r.i != c.i && self.solution_count < seek {
            self.prop_solution.push(r.id);

            let mut j = self.at(r.ri);
            while j.i != r.i {
                let cj = self.at(j.ci);
                self.cover_column(&cj);

                j = self.at(j.ri);
            }

            self.dance(k + 1, seek, print, rand);

            self.prop_solution.pop();

            let mut j = self.at(r.li);
            while j.i != r.i {
                let cj = self.at(j.ci);
                self.uncover_column(&cj);

                j = self.at(j.li);
            }

            r = self.at(r.di);
        }

        self.uncover_column(&c);

        if k == 0 {
            if self.solution_count == 1 && seek > 1 {
                self.uniq = Knowing::Yes;
            } else if self.solution_count == 0 {
                self.possible = Knowing::No;
            }
        }
    }

    fn choose_column(&self, rand: bool) -> Node {
        if rand {
            self.choose_column_randomly()
        } else {
            self.choose_column_well()
        }
    }

    fn choose_column_well(&self) -> Node {
        let mut s = usize::max_value();

        let h = self.at(0);
        let mut j = self.at(h.ri);
        let mut c = j;

        while j.i != h.i {
            if j.size < s {
                s = j.size;
                c = j;
            }
            j = self.at(j.ri);
        }

        c
    }

    fn choose_column_randomly(&self) -> Node {
        let mut s = usize::max_value();
        let h = self.at(0);
        let mut j = self.at(h.ri);
        let mut i: Vec<usize> = vec![];

        while j.i != h.i {
            if j.size == s {
                i.push(j.i);
            } else if j.size < s {
                s = j.size;
                i.clear();
                i.push(j.i);
            }
            j = self.at(j.ri);
        }
        let index = rand::thread_rng().gen_range(0, i.len());
        self.at(i[index])
    }

    fn cover_column(&mut self, c: &Node) {
        self.slab[c.ri].li = c.li;
        self.slab[c.li].ri = c.ri;

        let mut i = self.at(c.di);
        while i.i != c.i {
            let mut j = self.at(i.ri);
            while j.i != i.i {
                self.slab[j.di].ui = j.ui;
                self.slab[j.ui].di = j.di;
                self.slab[j.ci].size -= 1;

                j = self.at(j.ri);
            }
            i = self.at(i.di);
        }
    }

    fn uncover_column(&mut self, c: &Node) {
        let mut i = self.at(c.ui);
        while i.i != c.i {
            let mut j = self.at(i.li);
            while j.i != i.i {
                self.slab[j.di].ui = j.i;
                self.slab[j.ui].di = j.i;
                self.slab[j.ci].size += 1;

                j = self.at(j.li);
            }
            i = self.at(i.ui);
        }

        self.slab[c.ri].li = c.i;
        self.slab[c.li].ri = c.i;
    }

    fn print_horiz_line(&self, ls: &str, rs: &str, bm: &str, tm: &str, h: &str, sym_width: usize) {
        print!("{}", ls);
        for _ in 1..self.curtains {
            for _ in 1..self.belts {
                for _ in 0..sym_width {
                    print!("{}", h);
                }
                print!("{}", tm);
            }
            for _ in 0..sym_width {
                print!("{}", h);
            }
            print!("{}", bm);
        }
        for _ in 1..self.belts {
            for _ in 0..sym_width {
                print!("{}", h);
            }
            print!("{}", tm);
        }
        for _ in 0..sym_width {
            print!("{}", h);
        }
        println!("{}", rs);
    }

    fn print_solution(&self, sol: &Vec<[usize; 3]>) {
        let num = self.symbols();
        let mut a = vec![vec!["".to_string(); num]; num];
        let mut sym_width = 2;
        for s in sol.iter() {
            let [r, c, n] = s;
            let n = n.to_string();

            if sym_width < n.len() {
                sym_width = n.len();
            }

            a[r - 1][c - 1] = n;
        }

        for (r_i, r) in a.iter().enumerate() {
            if r_i == 0 {
                self.print_horiz_line("╔", "╗", "╦", "╤", "═", sym_width);
            } else if r_i % self.curtains == 0 {
                self.print_horiz_line("╠", "╣", "╬", "╪", "═", sym_width);
            } else {
                self.print_horiz_line("╟", "╢", "╫", "┼", "─", sym_width);
            }
            for (c_i, c) in r.iter().enumerate() {
                let mut cc = c.clone();
                for _ in 0..(sym_width - c.len()) {
                    cc.insert(0, ' ');
                }
                if c_i % self.belts == 0 {
                    print!("║");
                } else {
                    print!("│");
                }
                print!("{}", cc);
            }
            println!("║");
        }
        // Print the bottom border
        self.print_horiz_line("╚", "╝", "╩", "╧", "═", sym_width);
    }

    // fn print_column_counts(&self) {
    //     let h = self.at(0);
    //     let mut c = self.at(h.ri);
    //     while c.i != h.i {
    //         print!("{} ", c.size);
    //         c = self.at(c.ri);
    //     }
    //     println!("");
    // }

    // Sets prop_solution to a subset of some random solution, generating a random sudoku puzzle.
    fn random_puzzle(&mut self) {
        self.prop_solution = vec![];
        self.solve(1, false, true);
        self.prop_solution = self.solution.clone();

        rand::thread_rng().shuffle(&mut self.prop_solution);
        for i in (0..self.prop_solution.len()).rev() {
            let gone = self.prop_solution.remove(i);
            self.solve(2, false, false);
            if self.uniq == Knowing::No {
                self.prop_solution.push(gone);
            }
        }
    }

    fn prop_solution_string(&self) -> String {
        let nums = self.symbols();

        (1..=nums)
            .flat_map(|r| {
                (1..=nums).map(move |c| {
                    self.prop_solution
                        .iter()
                        .find(|e| e[0] == r && e[1] == c)
                        .map(|e| e[2].to_string())
                        .unwrap_or_else(|| ".".to_string())
                })
            })
            .collect()
    }
}

fn main() {
    // A 17-clue regular sudoku board
    // let v17 = vec![[1, 4, 8], [1, 6, 1], [2, 8, 4], [2, 9, 3], [3, 1, 5], [4, 5, 7], [4, 7, 8], [5, 7, 1], [6, 2, 2], [6, 5, 3], [7, 1, 6], [7, 8, 7], [7, 9, 5], [8, 3, 3], [8, 4, 4], [9, 4, 2], [9, 7, 6]];

    let mut sw = SudokuWeb::new(3, 3);

    // sw.random_puzzle();
    // sw.print_solution(&sw.prop_solution);
    // println!("{:?}", sw.prop_solution);

    let mut counts: Vec<usize> = vec![];
    // let mut min = usize::max_value();
    // let mut min_sol = sw.prop_solution.clone();

    let mut min = 81;
    let mut min_prop = sw.prop_solution.clone();
    for _ in 0..100 {
        sw.random_puzzle();
        println!("prop_solution string: {}", sw.prop_solution_string());
        let len = sw.prop_solution.len();
        if len < min {
            min = len;
            min_prop = sw.prop_solution.clone();
        }
        counts.push(len);
    }
    println!("Min hints: {}", min);
    sw.prop_solution = min_prop;
    sw.solve(2, false, false);
    sw.print_solution(&sw.prop_solution);
    sw.print_solution(&sw.solution);

    println!("prop_solution: {:?}", sw.prop_solution);
    println!("prop_solution string: {}", sw.prop_solution_string());

    // let mut count = 0;
    // while min > 19 {
    //     sw.random_puzzle();
    //     count += 1;
    //     let len = sw.prop_solution.len();
    //     counts.push(len);
    //     if len < min {
    //         min = len;
    //         min_sol = sw.prop_solution.clone();
    //         println!("[Try #{}] New min sudoku: {}", count, min);
    //         sw.print_solution(&min_sol);
    //     }
    //     if count % 1000 == 0 {
    //         println!("Generating the {}th sudoku", count);
    //     }
    // }

    println!("{:?}", counts);

    // let mine = vec![[5, 4, 1], [1, 3, 9], [1, 1, 4], [4, 4, 9], [3, 8, 5], [7, 8, 2], [1, 4, 5], [5, 3, 8], [9, 3, 3], [9, 2, 9], [2, 7, 4], [7, 3, 4], [6, 7, 3], [6, 9, 5], [7, 6, 8], [6, 3, 2], [2, 6, 1], [1, 5, 8], [4, 5, 5], [1, 9, 6], [6, 5, 4], [3, 1, 3], [8, 8, 9], [4, 7, 6], [8, 1, 8]];
    // sw.prop_solution = mine;
    // sw.print_solution(&sw.prop_solution);
    // sw.solve(2, false, false);
    // sw.print_solution(&sw.solution);
    // println!("Uniq? {:?}, Possible? {:?}", sw.uniq, sw.possible);

    // sw.solve(1, true, false);

    // // Find a puzzle that's valid for 3x2 and 2x3
    // let mut swa = SudokuWeb::new(2, 3);
    // let mut swb = SudokuWeb::new(3, 2);

    // swa.solve(1, false, true);
    // swb.prop_solution = swa.solution.clone();;
    // swb.solve(1, false, false);

    // while swb.possible != Knowing::Yes {
    //     swa.solve(1, false, true);
    //     swb.prop_solution = swa.solution.clone();;
    //     swb.solve(1, false, false);
    // }

    // swa.print_solution(&swa.solution);
    // swb.print_solution(&swb.solution);

    // swa.prop_solution = swa.solution.clone();
    // let os = swa.solution.clone();

    // rand::thread_rng().shuffle(&mut swa.prop_solution);
    // swb.prop_solution = swa.prop_solution.clone();

    // for i in (0..swa.prop_solution.len()).rev() {
    //     let gonea = swa.prop_solution.remove(i);
    //     let goneb = swb.prop_solution.remove(i);
    //     swa.solve(2, false, false);
    //     swb.solve(2, false, false);
    //     if swa.uniq == Knowing::No || swb.uniq == Knowing::No {
    //         if swa.uniq == Knowing::No {
    //             swa.solution = os.clone();
    //         }
    //         if swb.uniq == Knowing::No {
    //             swb.solution = os.clone();
    //         }
    //         swa.prop_solution.push(gonea);
    //         swb.prop_solution.push(goneb);
    //     }
    // }

    // swa.print_solution(&swa.solution);
    // swb.print_solution(&swb.solution);
    // swa.print_solution(&swa.prop_solution);
    // swb.print_solution(&swb.prop_solution);
}
